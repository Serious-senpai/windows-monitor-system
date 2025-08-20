pub mod providers;

use std::collections::VecDeque;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferrisetw::trace::{KernelTrace, TraceBuilder, TraceTrait};
use log::{debug, error, info};
use tokio::sync::{Mutex, RwLock, Semaphore, mpsc};
use tokio::task::{self, JoinHandle};
use tokio::time::timeout;
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
    _http_semaphore: Semaphore,
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
            _http_semaphore: Semaphore::new(5),
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

    async fn _send(
        self: Arc<Self>,
        events: &mut VecDeque<CapturedEventRecord>,
    ) -> Option<JoinHandle<()>> {
        let events_to_send = events.drain(..).collect::<Vec<CapturedEventRecord>>();
        match serde_json::to_vec(&events_to_send) {
            Ok(data) => {
                let before = data.len();
                match zstd::bulk::compress(&data, self._configuration.zstd_compression_level) {
                    Ok(compressed) => Some(tokio::spawn(async move {
                        let after = compressed.len();
                        info!("Compressed events from {before} to {after} bytes");
                        if let Ok(_) = self._http_semaphore.acquire().await {
                            if let Err(e) = self
                                ._http
                                .api()
                                .post("/trace")
                                .body(compressed)
                                .send()
                                .await
                            {
                                error!("Failed to send trace event to server: {e}");
                            }
                        }
                    })),
                    Err(e) => {
                        error!("Failed to compress payload: {e}");
                        None
                    }
                }
            }
            Err(e) => {
                error!("Unable to serialize events: {e}");
                None
            }
        }
    }

    async fn _poll_and_send(
        self: Arc<Self>,
        mut receiver: mpsc::UnboundedReceiver<Event>,
    ) -> (mpsc::UnboundedReceiver<Event>, Option<JoinHandle<()>>) {
        let mut events = VecDeque::new();
        let mut last_task = None;
        loop {
            match timeout(Duration::from_secs(1), receiver.recv()).await {
                Ok(Some(event)) => {
                    events.push_back(CapturedEventRecord {
                        event,
                        system: SystemInfo::fetch().await,
                    });

                    if events.len() >= self._configuration.events_per_request {
                        last_task = self.clone()._send(&mut events).await;
                    }
                }
                Ok(None) => break (receiver, last_task),
                Err(_) => {
                    if !events.is_empty() {
                        last_task = self.clone()._send(&mut events).await;
                    }
                }
            }
        }
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
        let process_handle_self = self.clone();
        let process_handle =
            tokio::spawn(async move { process_handle_self.clone()._poll_and_send(receiver).await });

        let (trace, handle) = Self::_trace_builder(sender)
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start kernel trace: {e:?}")))?;

        self._set_trace(trace).await;

        // Process trace in a blocking thread; this call will block until the trace stops.
        task::spawn_blocking(move || KernelTrace::process_from_handle(handle))
            .await?
            .map_err(|e| RuntimeError::new(format!("Kernel trace error: {e:?}")))?;

        let process_result = process_handle
            .await
            .map_err(|e| RuntimeError::new(format!("Unable to reobtain receiver: {e:?}")))?;

        receiver = process_result.0;
        receiver.close();

        if let Some(task) = process_result.1 {
            let _ = task.await;
        }

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
        self._http_semaphore.close();
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
