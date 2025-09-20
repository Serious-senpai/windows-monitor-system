use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use log::error;
use tokio::fs;
use tokio::sync::{Mutex, SetOnce};
use tokio::time::sleep;

use crate::backup::Backup;
use crate::http::HttpClient;
use crate::module::Module;

pub struct BackupSender {
    _backup: Arc<Mutex<Backup>>,
    _http: Arc<HttpClient>,
    _stopped: Arc<SetOnce<()>>,
    _last_backup_switch: Mutex<Instant>,
}

impl BackupSender {
    pub fn new(backup: Arc<Mutex<Backup>>, http: Arc<HttpClient>) -> Self {
        Self {
            _backup: backup,
            _http: http,
            _stopped: Arc::new(SetOnce::new()),
            _last_backup_switch: Mutex::new(Instant::now()),
        }
    }
}

#[async_trait]
impl Module for BackupSender {
    type EventType = ();

    fn name(&self) -> &str {
        "BackupSender"
    }

    fn stopped(&self) -> Arc<SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        sleep(Duration::from_secs(5)).await;
    }

    async fn handle(
        self: Arc<Self>,
        _: Self::EventType,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Err(e) =
            Backup::upload(self._backup.clone(), self._http.clone(), self.stopped()).await
        {
            error!("Unable to upload backup: {e}");
        }

        let mut backup = self._backup.lock().await;
        let mut last_backup_switch = self._last_backup_switch.lock().await;

        if let Ok(metadata) = fs::metadata(backup.path()).await
            && metadata.len() < (5 << 20)
            && last_backup_switch.elapsed() < Duration::from_secs(60)
        {
            // We switch backup files at most once every 1 minute or if the file exceeds 5 MB
        } else {
            backup.switch_backup().await;
            *last_backup_switch = Instant::now();
        }

        Ok(())
    }
}
