use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use ferrisetw::trace::{KernelTrace, TraceTrait};
use log::{debug, error};
use tokio::sync::{Mutex, RwLock};
use tokio::task;

use crate::error::RuntimeError;
use crate::module::Module;
use crate::module::tracer::providers::ProviderWrapper;
use crate::module::tracer::providers::file::FileProviderWrapper;
use crate::module::tracer::providers::image::ImageProviderWrapper;
use crate::module::tracer::providers::process::ProcessProviderWrapper;
use crate::module::tracer::providers::tcpip::TcpIpProviderWrapper;
use crate::module::tracer::providers::udpip::UdpIpProviderWrapper;

pub mod data;
pub mod providers;

pub struct EventTracer {
    _trace: RwLock<Option<KernelTrace>>,
    _running: Mutex<bool>,
}

#[async_trait]
impl Module for EventTracer {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {
            _trace: RwLock::new(None),
            _running: Mutex::new(false),
        }
    }

    fn name(&self) -> &str {
        "EventTracer"
    }

    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        {
            let mut running = self._running.lock().await;
            if *running {
                return Err(RuntimeError::new("EventTracer is already running").into());
            }

            *running = true;
        }

        debug!("Running EventTracer");
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
            tracer = wrapper.attach(tracer);
        }

        let (trace, handle) = tracer
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start kernel trace: {e:?}")))?;

        {
            let mut self_trace = self._trace.write().await;
            *self_trace = Some(trace);
        }

        // Process trace in a blocking thread; this call will block until the trace stops.
        task::spawn_blocking(move || KernelTrace::process_from_handle(handle))
            .await?
            .map_err(|e| RuntimeError::new(format!("Kernel trace error: {e:?}")))?;

        debug!("EventTracer completed");

        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
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
