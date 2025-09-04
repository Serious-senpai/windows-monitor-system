pub mod enricher;
pub mod providers;

use std::error::Error;
use std::sync::{Arc, Mutex as BlockingMutex};
use std::time::Duration;

use async_trait::async_trait;
use ferrisetw::trace::{KernelTrace, TraceBuilder, TraceTrait, stop_trace_by_name};
use log::{debug, error};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task;
use wm_common::error::RuntimeError;
use wm_common::schema::event::CapturedEventRecord;

use crate::backup::Backup;
use crate::configuration::Configuration;
use crate::module::Module;
use crate::module::tracer::enricher::BlockingEventEnricher;
use crate::module::tracer::providers::ProviderWrapper;
use crate::module::tracer::providers::file::FileProviderWrapper;
use crate::module::tracer::providers::image::ImageProviderWrapper;
use crate::module::tracer::providers::process::ProcessProviderWrapper;
use crate::module::tracer::providers::tcpip::TcpIpProviderWrapper;
use crate::module::tracer::providers::udpip::UdpIpProviderWrapper;

pub struct EventTracer {
    _configuration: Arc<Configuration>,
    _sender: mpsc::Sender<Arc<CapturedEventRecord>>,
    _trace: RwLock<Option<KernelTrace>>,
    _running: Mutex<bool>,
    _backup: Arc<Mutex<Backup>>,
    _enricher: Arc<BlockingMutex<BlockingEventEnricher>>,
}

impl EventTracer {
    pub async fn async_new(
        configuration: Arc<Configuration>,
        sender: mpsc::Sender<Arc<CapturedEventRecord>>,
        backup: Arc<Mutex<Backup>>,
    ) -> Self
    where
        Self: Sized,
    {
        Self {
            _configuration: configuration.clone(),
            _sender: sender,
            _trace: RwLock::new(None),
            _running: Mutex::new(false),
            _backup: backup,
            _enricher: Arc::new(BlockingMutex::new(
                BlockingEventEnricher::async_new(Duration::from_secs_f64(
                    configuration.system_refresh_interval_seconds,
                ))
                .await,
            )),
        }
    }

    fn _trace_builder(self: &Arc<Self>) -> TraceBuilder<KernelTrace> {
        let mut tracer = KernelTrace::new().named(self._configuration.trace_name.clone());
        let wrappers: Vec<Arc<dyn ProviderWrapper>> = vec![
            Arc::new(FileProviderWrapper::new()),
            Arc::new(ImageProviderWrapper::new()),
            Arc::new(ProcessProviderWrapper::new()),
            Arc::new(TcpIpProviderWrapper::new()),
            Arc::new(UdpIpProviderWrapper::new()),
            // Add other providers here as needed
        ];
        for wrapper in wrappers {
            tracer = wrapper.attach(
                tracer,
                self._sender.clone(),
                self._enricher.clone(),
                self._backup.clone(),
            );
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

        let _ = stop_trace_by_name(&self._configuration.trace_name);
        let (trace, handle) = self
            ._trace_builder()
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start kernel trace: {e:?}")))?;

        self._set_trace(trace).await;

        // Process trace in a blocking thread; this call will block until the trace stops.
        task::spawn_blocking(move || KernelTrace::process_from_handle(handle))
            .await?
            .map_err(|e| RuntimeError::new(format!("Kernel trace error: {e:?}")))?;

        debug!("EventTracer completed");

        Ok(())
    }

    async fn stop(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut running = self._running.lock().await;
        if !*running {
            return Err(RuntimeError::new("EventTracer is not running").into());
        }

        debug!("Stopping EventTracer");

        let mut self_trace = self._trace.write().await;
        if let Some(trace) = self_trace.take() {
            trace
                .stop()
                .map_err(|e| RuntimeError::new(format!("Error stopping EventTracer: {e:?}")))?;
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
