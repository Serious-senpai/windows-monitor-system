use std::sync::Arc;

use log::{error, info};

use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;
use crate::module::tracer::EventTracer;

pub struct Agent {
    _config: Arc<Configuration>,
    _modules: Vec<Arc<dyn Module>>,
    _http: Arc<HttpClient>,
}

impl Agent {
    pub fn new(config: Arc<Configuration>) -> Self {
        let http = Arc::new(HttpClient::new(&config));
        Self {
            _config: config.clone(),
            _modules: vec![Arc::new(EventTracer::new(config, http.clone()))],
            _http: http,
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
                if let Err(e) = ptr.clone().run().await {
                    error!("Module {} completed with error: {e}", ptr.name());
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
            if let Err(e) = module.clone().stop().await {
                error!("Module {} stopped with error: {e}", module.name());
            }
        }
    }
}
