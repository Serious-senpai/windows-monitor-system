pub mod providers;

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use ferrisetw::trace::{KernelTrace, TraceBuilder, TraceTrait};
use log::{debug, error, trace};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task;
use wm_common::error::RuntimeError;
use wm_common::schema::{CapturedEventRecord, Event};
use wm_common::sysinfo::SystemInfo;

use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;
use crate::module::tracer::providers::ProviderWrapper;
use crate::module::tracer::providers::file::FileProviderWrapper;
use crate::module::tracer::providers::image::ImageProviderWrapper;
use crate::module::tracer::providers::process::ProcessProviderWrapper;
use crate::module::tracer::providers::tcpip::TcpIpProviderWrapper;
use crate::module::tracer::providers::udpip::UdpIpProviderWrapper;

pub struct EventTracer {
    _configuration: Arc<Configuration>,
    _http: Arc<HttpClient>,
    _trace: RwLock<Option<KernelTrace>>,
    _running: Mutex<bool>,
}

impl EventTracer {
    async fn _process_event(self: Arc<Self>, event: Event, buffer_length: usize) {
        let record = CapturedEventRecord {
            event,
            system: SystemInfo::fetch().await,
            buffer_length,
        };

        trace!("{}", serde_json::to_string(&record).unwrap());
        // Send the record to the configured server
        let client = self._http.client();
        let server_url = self._configuration.server.join("/trace").unwrap();
        if let Err(e) = client.post(server_url).json(&record).send().await {
            error!("Failed to send trace event to server: {e}");
        }
    }

    fn _trace_builder(sender: mpsc::UnboundedSender<Event>) -> TraceBuilder<KernelTrace> {
        let mut tracer = KernelTrace::new().named("Windows Monitor Event Tracer".into());
        let wrappers: Vec<Arc<dyn ProviderWrapper>> = vec![
            Arc::new(FileProviderWrapper::new()),
            Arc::new(ImageProviderWrapper::new()),
            Arc::new(ProcessProviderWrapper::new()),
            Arc::new(TcpIpProviderWrapper::new()),
            Arc::new(UdpIpProviderWrapper::new()),
            // Add other providers here as needed
        ];
        for wrapper in wrappers {
            tracer = wrapper.attach(tracer, sender.clone());
        }

        tracer
    }

    async fn _set_trace(&self, trace: KernelTrace) {
        let mut self_trace = self._trace.write().await;
        *self_trace = Some(trace);
    }
}

#[async_trait]
impl Module for EventTracer {
    fn name(&self) -> &str {
        "EventTracer"
    }

    async fn run(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        {
            let mut running = self._running.lock().await;
            if *running {
                return Err(RuntimeError::new("EventTracer is already running").into());
            }

            *running = true;
        }

        debug!("Running EventTracer");

        let (sender, mut receiver) = mpsc::unbounded_channel();
        let self_cloned = self.clone();
        let process_handle = tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Some(event) => {
                        self_cloned
                            .clone()
                            ._process_event(event, receiver.len())
                            .await
                    }
                    None => break receiver,
                }
            }
        });

        let tracer = Self::_trace_builder(sender);

        let (trace, handle) = tracer
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start kernel trace: {e:?}")))?;

        self._set_trace(trace).await;

        // Process trace in a blocking thread; this call will block until the trace stops.
        task::spawn_blocking(move || KernelTrace::process_from_handle(handle))
            .await?
            .map_err(|e| RuntimeError::new(format!("Kernel trace error: {e:?}")))?;

        receiver = process_handle
            .await
            .map_err(|e| RuntimeError::new(format!("Unable to reobtain receiver: {e:?}")))?;
        receiver.close();

        debug!("EventTracer completed");

        Ok(())
    }

    async fn stop(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut running = self._running.lock().await;
        if !*running {
            return Err(RuntimeError::new("EventTracer is not running").into());
        }

        debug!("Stopping EventTracer");

        {
            let mut self_trace = self._trace.write().await;
            if let Some(trace) = self_trace.take() {
                trace
                    .stop()
                    .map_err(|e| RuntimeError::new(format!("Error stopping EventTracer: {e:?}")))?;
            }
        }

        *running = false;
        debug!("EventTracer stopped");

        Ok(())
    }
}

impl Drop for EventTracer {
    fn drop(&mut self) {
        if let Some(trace) = self._trace.get_mut().take()
            && let Err(e) = trace.stop()
        {
            error!("Error stopping EventTracer: {e:?}");
        }

        *self._running.get_mut() = false;
    }
}

impl EventTracer {
    pub fn new(configuration: Arc<Configuration>, http: Arc<HttpClient>) -> Self
    where
        Self: Sized,
    {
        Self {
            _configuration: configuration,
            _http: http,
            _trace: RwLock::new(None),
            _running: Mutex::new(false),
        }
    }
}
