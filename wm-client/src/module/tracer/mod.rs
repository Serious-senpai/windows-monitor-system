pub mod enricher;
pub mod providers;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferrisetw::trace::{KernelTrace, TraceBuilder, TraceError, TraceTrait, stop_trace_by_name};
use parking_lot::Mutex as BlockingMutex;
use tokio::sync::{Mutex, SetOnce, mpsc};
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
use crate::module::tracer::providers::registry::RegistryProviderWrapper;
use crate::module::tracer::providers::tcpip::TcpIpProviderWrapper;
use crate::module::tracer::providers::udpip::UdpIpProviderWrapper;

pub struct EventTracer {
    _config: Arc<Configuration>,
    _sender: mpsc::Sender<Arc<CapturedEventRecord>>,
    _trace: Mutex<Option<KernelTrace>>,
    _trace_task: Mutex<Option<task::JoinHandle<Result<(), TraceError>>>>,
    _stopped: Arc<SetOnce<()>>,
    _backup: Arc<Mutex<Backup>>,
    _enricher: Arc<BlockingMutex<BlockingEventEnricher>>,
}

impl EventTracer {
    pub async fn async_new(
        config: Arc<Configuration>,
        sender: mpsc::Sender<Arc<CapturedEventRecord>>,
        backup: Arc<Mutex<Backup>>,
    ) -> Self
    where
        Self: Sized,
    {
        Self {
            _config: config.clone(),
            _sender: sender,
            _trace: Mutex::new(None),
            _trace_task: Mutex::new(None),
            _stopped: Arc::new(SetOnce::new()),
            _backup: backup,
            _enricher: Arc::new(BlockingMutex::new(
                BlockingEventEnricher::async_new(Duration::from_secs_f64(
                    config.system_refresh_interval_seconds,
                ))
                .await,
            )),
        }
    }

    fn _trace_builder(self: &Arc<Self>) -> TraceBuilder<KernelTrace> {
        let mut tracer = KernelTrace::new().named(self._config.trace_name.clone());
        let wrappers: Vec<Arc<dyn ProviderWrapper>> = vec![
            Arc::new(FileProviderWrapper {}),
            Arc::new(ImageProviderWrapper {}),
            Arc::new(ProcessProviderWrapper {}),
            Arc::new(RegistryProviderWrapper {}),
            Arc::new(TcpIpProviderWrapper {}),
            Arc::new(UdpIpProviderWrapper {}),
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
}

#[async_trait]
impl Module for EventTracer {
    type EventType = ();

    fn name(&self) -> &str {
        "EventTracer"
    }

    fn stopped(&self) -> Arc<SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        self._stopped.wait().await;
    }

    async fn handle(
        self: Arc<Self>,
        _: Self::EventType,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn before_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let _ = stop_trace_by_name(&self._config.trace_name);
        let (trace, handle) = self
            ._trace_builder()
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start kernel trace: {e:?}")))?;

        *self._trace.lock().await = Some(trace);
        self._trace_task
            .lock()
            .await
            .replace(task::spawn_blocking(move || {
                KernelTrace::process_from_handle(handle)
            }));

        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut self_trace = self._trace.lock().await;
        if let Some(trace) = self_trace.take() {
            trace
                .stop()
                .map_err(|e| RuntimeError::new(format!("Error stopping EventTracer: {e:?}")))?;
        }

        let mut trace_task = self._trace_task.lock().await;
        if let Some(handle) = trace_task.take() {
            handle
                .await?
                .map_err(|e| RuntimeError::new(format!("Kernel trace error: {e:?}")))?;
        }
        Ok(())
    }
}
