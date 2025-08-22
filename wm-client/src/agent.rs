use std::path::PathBuf;
use std::sync::Arc;

use log::{error, info};
use tokio::fs;
use tokio::sync::{Mutex, mpsc};
use wm_common::credential::CredentialManager;

use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;
use crate::module::connector::Connector;
use crate::module::tracer::EventTracer;

pub struct Agent {
    _config: Arc<Configuration>,
    _modules: Vec<Arc<dyn Module>>,
    _http: Arc<HttpClient>,
}

impl Agent {
    pub async fn async_new(config: Arc<Configuration>, password: &str) -> Self {
        let _ = fs::create_dir_all(&config.backup_directory).await;

        let mut index = 0;
        while Self::_get_log_file_path(config.clone(), index).exists() {
            index += 1;
            // if index == 1000 {
            //     panic!("Too many backup files");
            // }
        }

        let backup_path = Self::_get_log_file_path(config.clone(), index);
        let backup = Arc::new(Mutex::new(
            fs::File::create(&backup_path)
                .await
                .expect("Failed to create backup file"),
        ));

        let http = Arc::new(HttpClient::new(&config, password));
        let (sender, receiver) = mpsc::channel(config.message_queue_limit);

        Self {
            _config: config.clone(),
            _modules: vec![
                Arc::new(EventTracer::async_new(config.clone(), sender, backup.clone()).await),
                Arc::new(
                    Connector::async_new(config.clone(), receiver, backup.clone(), http.clone())
                        .await,
                ),
            ],
            _http: http,
        }
    }

    fn _get_log_file_path(configuration: Arc<Configuration>, index: i32) -> PathBuf {
        configuration
            .backup_directory
            .join(format!("backup-{index}.jsonl"))
    }

    pub async fn read_password(config: &Configuration) -> String {
        let data = CredentialManager::read(&format!("{}\0", config.windows_credential_manager_key))
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to read password from Windows Credential Manager. Have you set it yet?"
                )
            });
        String::from_utf8_lossy(&data).to_string()
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
