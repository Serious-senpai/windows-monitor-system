pub mod connector;
pub mod scanner;
pub mod tracer;

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;

#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &str;
    async fn run(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn stop(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>>;
}
