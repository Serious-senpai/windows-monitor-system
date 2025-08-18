pub mod tracer;

use std::error::Error;

use async_trait::async_trait;

#[async_trait]
pub trait Module: Send + Sync {
    fn new() -> Self
    where
        Self: Sized;

    fn name(&self) -> &str;
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn stop(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
}
