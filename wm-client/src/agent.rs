use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use log::{error, info};
use tokio::sync::{Mutex, SetOnce, mpsc};
use tokio::task::JoinHandle;

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
    _stopped: Arc<SetOnce<()>>,
    _backup: Arc<Mutex<Backup>>,
    _http: Arc<HttpClient>,
    _tasks: Arc<Mutex<Vec<JoinHandle<Result<(), Box<dyn Error + Send + Sync>>>>>>,
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
            _stopped: Arc::new(SetOnce::new()),
            _backup: backup,
            _http: http,
            _tasks: Arc::new(Mutex::new(vec![])),
        }
    }
}

#[async_trait]
impl Module for Agent {
    type EventType = ();

    fn name(&self) -> &str {
        "Agent"
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
        info!(
            "Starting agent with configuration: {}",
            serde_json::to_string(&self._config).unwrap()
        );

        let mut tasks = self._tasks.lock().await;
        tasks.push(tokio::spawn(self._tracer.clone().run()));
        tasks.push(tokio::spawn(self._backup_sender.clone().run()));
        tasks.push(tokio::spawn(self._connector.clone().run()));

        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        self._tracer.stop();
        self._backup_sender.stop();
        self._connector.stop();

        let mut tasks = self._tasks.lock().await;
        for task in tasks.drain(..) {
            match task.await {
                Ok(Err(e)) => error!("Task failed: {e}"),
                Err(e) => error!("Task panicked: {e}"),
                _ => {}
            }
        }

        Ok(())
    }
}
