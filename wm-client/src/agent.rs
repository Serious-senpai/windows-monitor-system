use std::sync::Arc;

use log::{error, info};
use tokio::sync::{Mutex, mpsc};

use crate::backup::Backup;
use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;
use crate::module::backup::BackupSender;
use crate::module::connector::Connector;
use crate::module::tracer::EventTracer;

pub struct Agent {
    // Module list
    _tracer: Arc<EventTracer>,
    _backup_sender: Arc<BackupSender>,
    _connector: Arc<Connector>,

    _config: Arc<Configuration>,
    _http: Arc<HttpClient>,
    _backup: Arc<Mutex<Backup>>,
}

impl Agent {
    pub async fn async_new(config: Arc<Configuration>, password: &str) -> Self {
        let backup = Arc::new(Mutex::new(Backup::async_new(config.clone()).await));

        let http = Arc::new(HttpClient::new(&config, password));
        let (sender, receiver) = mpsc::channel(config.message_queue_limit);

        Self {
            _tracer: Arc::new(EventTracer::async_new(config.clone(), sender, backup.clone()).await),
            _backup_sender: Arc::new(BackupSender::new(backup.clone(), http.clone())),
            _connector: Connector::new(config.clone(), receiver, backup.clone(), http.clone()),
            _config: config.clone(),
            _http: http,
            _backup: backup,
        }
    }

    pub async fn run(&self) {
        info!(
            "Starting agent with configuration: {}",
            serde_json::to_string(&self._config).unwrap()
        );

        let mut tasks = vec![];
        tasks.push(tokio::spawn(self._tracer.clone().run()));
        tasks.push(tokio::spawn(self._backup_sender.clone().run()));
        tasks.push(tokio::spawn(self._connector.clone().run()));

        for task in tasks {
            if let Err(e) = task.await {
                error!("Task failed with error: {e}");
            }
        }

        info!("Agent run completed");
    }

    pub async fn stop(&self) {
        self._tracer.stop();
        self._backup_sender.stop();
        self._connector.stop();
        self._backup.lock().await.flush().await;
    }
}
