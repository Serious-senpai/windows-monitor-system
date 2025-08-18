use std::sync::Arc;

use log::{error, info};
use tokio::sync::RwLock;
use wm_common::module::Module;
use wm_common::module::tracer::EventTracer;

use crate::configuration::Configuration;

pub struct Agent {
    _config: Arc<Configuration>,
    _modules: Vec<Arc<RwLock<dyn Module>>>,
}

impl Agent {
    pub fn new(config: Arc<Configuration>) -> Self {
        Self {
            _config: config,
            _modules: vec![Arc::new(RwLock::new(EventTracer::new()))],
        }
    }

    pub async fn run(&self) {
        info!(
            "Starting agent with configuration: {}",
            serde_json::to_string(&self._config).unwrap()
        );

        let mut tasks = vec![];
        for module in &self._modules {
            let ptr = module.clone();

            let task = tokio::spawn(async move {
                let ptr_owned = ptr.read_owned().await;
                if let Err(e) = ptr_owned.run().await {
                    error!("Module {} completed with error: {e}", ptr_owned.name());
                }
            });

            tasks.push(task);
        }

        for task in tasks {
            if let Err(e) = task.await {
                error!("Task failed with error: {e}");
            }
        }

        info!("Agent run completed");
    }

    pub async fn stop(&self) {
        for module in &self._modules {
            let ptr_owned = module.read().await;
            if let Err(e) = ptr_owned.stop().await {
                error!("Module {} stopped with error: {e}", ptr_owned.name());
            }
        }
    }
}
