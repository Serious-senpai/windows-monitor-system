pub mod backup;
pub mod connector;
pub mod scanner;
pub mod tracer;

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error, info};
use tokio::sync::SetOnce;

#[async_trait]
pub trait Module: Send + Sync {
    type EventType;

    fn name(&self) -> &str;
    fn stopped(&self) -> Arc<SetOnce<()>>;

    async fn listen(self: Arc<Self>) -> Self::EventType;
    async fn handle(
        self: Arc<Self>,
        event: Self::EventType,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    async fn before_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn run(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        debug!("Running before_hook for module {}", self.name());
        self.clone().before_hook().await?;

        info!("Running module {}", self.name());
        while self.stopped().get().is_none() {
            let stopped = self.stopped();
            let event = tokio::select! {
                _ = stopped.wait() => break,
                event = self.clone().listen() => event,
            };

            self.clone().handle(event).await?;
        }

        debug!("Running after_hook for module {}", self.name());
        self.clone().after_hook().await?;

        info!("Module {} completed", self.name());
        Ok(())
    }

    fn stop(&self) {
        info!("Stopping module {}", self.name());
        if let Err(e) = self.stopped().set(()) {
            error!("Error stopping module {}: {e}", self.name());
        } else {
            info!("Module {} stopped", self.name());
        }
    }
}
