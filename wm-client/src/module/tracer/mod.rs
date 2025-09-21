pub mod enricher;
pub mod providers;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferrisetw::native::TraceHandle;
use ferrisetw::trace::{
    KernelTrace, TraceBuilder, TraceError, TraceTrait, UserTrace, stop_trace_by_name,
};
use parking_lot::Mutex as BlockingMutex;
use tokio::sync::{Mutex, SetOnce, mpsc};
use tokio::task;
use wm_common::error::RuntimeError;
use wm_common::schema::event::CapturedEventRecord;

use crate::backup::Backup;
use crate::configuration::Configuration;
use crate::module::Module;
use crate::module::tracer::enricher::BlockingEventEnricher;
use crate::module::tracer::providers::file::FileProviderWrapper;
use crate::module::tracer::providers::image::ImageProviderWrapper;
use crate::module::tracer::providers::process::ProcessProviderWrapper;
use crate::module::tracer::providers::registry::RegistryProviderWrapper;
use crate::module::tracer::providers::tcpip::TcpIpProviderWrapper;
use crate::module::tracer::providers::udpip::UdpIpProviderWrapper;
use crate::module::tracer::providers::{KernelProviderWrapper, UserProviderWrapper};

struct _TraceTask<T> {
    _trace: T,
    _task: task::JoinHandle<Result<(), TraceError>>,
}

impl<T> _TraceTask<T>
where
    T: TraceTrait,
{
    fn start(trace: T, handle: TraceHandle) -> Self {
        let task = task::spawn_blocking(move || T::process_from_handle(handle));

        Self {
            _trace: trace,
            _task: task,
        }
    }

    async fn stop(self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self._trace
            .stop()
            .map_err(|e| RuntimeError::new(format!("Error stopping trace: {e:?}")))?;

        self._task
            .await?
            .map_err(|e| RuntimeError::new(format!("Tracing thread error: {e:?}")))?;

        Ok(())
    }
}

pub struct EventTracer {
    _config: Arc<Configuration>,
    _sender: mpsc::Sender<Arc<CapturedEventRecord>>,
    _trace: Mutex<Option<(_TraceTask<KernelTrace>, _TraceTask<UserTrace>)>>,
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

    fn _kernel_trace(self: &Arc<Self>) -> TraceBuilder<KernelTrace> {
        let mut builder = KernelTrace::new().named(self._config.trace_name.kernel.clone());
        let wrappers: Vec<Arc<dyn KernelProviderWrapper>> = vec![
            Arc::new(FileProviderWrapper {}),
            Arc::new(ImageProviderWrapper {}),
            Arc::new(ProcessProviderWrapper {}),
            Arc::new(RegistryProviderWrapper {}),
            Arc::new(TcpIpProviderWrapper {}),
            Arc::new(UdpIpProviderWrapper {}),
            // Add other providers here as needed
        ];

        for wrapper in wrappers {
            builder = wrapper.attach(
                builder,
                self._sender.clone(),
                self._enricher.clone(),
                self._backup.clone(),
            );
        }

        builder
    }

    fn _user_trace(self: &Arc<Self>) -> TraceBuilder<UserTrace> {
        let mut builder = UserTrace::new().named(self._config.trace_name.user.clone());
        let wrappers: Vec<Arc<dyn UserProviderWrapper>> = vec![
            // Add user provider wrappers here as needed
        ];

        for wrapper in wrappers {
            builder = wrapper.attach(
                builder,
                self._sender.clone(),
                self._enricher.clone(),
                self._backup.clone(),
            );
        }

        builder
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
        let _ = stop_trace_by_name(&self._config.trace_name.kernel);
        let _ = stop_trace_by_name(&self._config.trace_name.user);

        let kernel = self
            ._kernel_trace()
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start kernel trace: {e:?}")))?;

        let user = self
            ._user_trace()
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start user trace: {e:?}")))?;

        *self._trace.lock().await = Some((
            _TraceTask::start(kernel.0, kernel.1),
            _TraceTask::start(user.0, user.1),
        ));

        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut self_trace = self._trace.lock().await;
        if let Some((kernel, user)) = self_trace.take() {
            kernel.stop().await?;
            user.stop().await?;
        }

        Ok(())
    }
}
