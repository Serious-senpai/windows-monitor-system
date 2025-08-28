use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_compression::Level;
use async_compression::tokio::bufread::ZstdEncoder;
use async_trait::async_trait;
use log::{debug, error};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock, Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use wm_common::error::RuntimeError;
use wm_common::schema::event::CapturedEventRecord;

use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;

pub struct Connector {
    _configuration: Arc<Configuration>,
    _receiver: Mutex<mpsc::Receiver<Arc<CapturedEventRecord>>>,
    _running: RwLock<bool>,
    _backup: Arc<Mutex<fs::File>>,

    _http: Arc<HttpClient>,
    _http_semaphore: Semaphore,

    _errors_count: Arc<RwLock<usize>>,
    _reconnect_task: JoinHandle<()>,
}

impl Connector {
    pub async fn async_new(
        configuration: Arc<Configuration>,
        receiver: mpsc::Receiver<Arc<CapturedEventRecord>>,
        backup: Arc<Mutex<fs::File>>,
        http: Arc<HttpClient>,
    ) -> Self
    where
        Self: Sized,
    {
        let concurrency_limit = configuration.event_post.concurrency_limit;
        let errors_count = Arc::new(RwLock::new(0));

        let http_cloned = http.clone();
        let errors_count_cloned = errors_count.clone();
        let reconnect_task = tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(5)).await;

                if Self::_static_disconnected(errors_count_cloned.clone(), concurrency_limit).await
                {
                    debug!("Attempting to reconnect to server...");
                    if let Ok(response) = http_cloned.api().get("/health-check").send().await
                        && response.status() == 204
                    {
                        *errors_count_cloned.write().await = 0;
                    }
                }
            }
        });

        Self {
            _configuration: configuration.clone(),
            _receiver: Mutex::new(receiver),
            _running: RwLock::new(false),
            _backup: backup,
            _http: http,
            _http_semaphore: Semaphore::new(concurrency_limit),
            _errors_count: errors_count,
            _reconnect_task: reconnect_task,
        }
    }

    async fn _static_disconnected(errors_count: Arc<RwLock<usize>>, limit: usize) -> bool {
        *errors_count.read().await == limit
    }

    async fn _disconnected(&self) -> bool {
        Self::_static_disconnected(
            self._errors_count.clone(),
            self._configuration.event_post.concurrency_limit,
        )
        .await
    }

    async fn _send_payload_utils(self: Arc<Self>, raw_payload: &mut Vec<u8>) {
        if raw_payload.len() == 1 {
            return;
        }

        raw_payload.pop(); // Remove trailing comma
        raw_payload.push(b']');

        let mut compressor = ZstdEncoder::with_quality(
            raw_payload.as_slice(),
            Level::Precise(self._configuration.zstd_compression_level),
        );

        // TODO: Allocate a certain number of buffers beforehand and reuse them via a pool with size = semaphore limit.
        let mut compressed = vec![];

        if let Err(e) = compressor.read_to_end(&mut compressed).await {
            error!("Unable to compress data: {e}");
        } else {
            let mut write_to_backup = self._disconnected().await;
            if !write_to_backup && let Ok(_) = self._http_semaphore.acquire().await {
                debug!(
                    "Sending {} bytes of uncompressed data (compressed to {} bytes)",
                    raw_payload.len(),
                    compressed.len()
                );

                if let Err(e) = self
                    ._http
                    .api()
                    .post("/count")
                    .body(compressed)
                    .send()
                    .await
                {
                    error!(
                        "Failed to send trace event to server: {e:?}, writing to backup instead"
                    );

                    let mut errors_count = self._errors_count.write().await;
                    *errors_count =
                        (*errors_count + 1).min(self._configuration.event_post.concurrency_limit);
                    write_to_backup = true;
                }
            }

            if write_to_backup {
                debug!(
                    "Backing up {} bytes of uncompressed data",
                    raw_payload.len()
                );
                let mut backup = self._backup.lock().await;

                if let Err(e) = backup.write(raw_payload).await {
                    error!("Failed to backup data: {e}");
                }
                let _ = backup.write(b"\n").await;
            }
        }

        raw_payload.clear();
        raw_payload.push(b'[');
    }
}

#[async_trait]
impl Module for Connector {
    fn name(&self) -> &str {
        "Connector"
    }

    async fn run(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        {
            let mut running = self._running.write().await;
            if *running {
                return Err(RuntimeError::new("Connector is already running").into());
            }

            *running = true;
        }

        debug!("Running Connector");

        let mut raw_payload =
            Vec::with_capacity(self._configuration.event_post.accumulated_batch_threshold);
        raw_payload.push(b'[');

        let mut receiver = self._receiver.lock().await;
        loop {
            if !*self._running.read().await {
                break;
            }

            match timeout(Duration::from_secs(1), receiver.recv()).await {
                Ok(Some(event)) => match serde_json::to_vec(&event) {
                    Ok(payload) => {
                        if raw_payload.len() + payload.len() + 1
                            > self._configuration.event_post.accumulated_batch_threshold
                        {
                            self.clone()._send_payload_utils(&mut raw_payload).await;
                        }

                        raw_payload.extend_from_slice(&payload);
                        raw_payload.push(b',');
                    }
                    Err(e) => {
                        error!("Failed to serialize {event:?}: {e}");
                    }
                },
                Ok(None) => break,
                Err(_) => {
                    self.clone()._send_payload_utils(&mut raw_payload).await;
                }
            }
        }

        debug!("Connector completed");

        Ok(())
    }

    async fn stop(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut running = self._running.write().await;
        if !*running {
            return Err(RuntimeError::new("Connector is not running").into());
        }

        debug!("Stopping Connector");

        *running = false;
        debug!("Connector stopped");

        Ok(())
    }
}

impl Drop for Connector {
    fn drop(&mut self) {
        *self._running.get_mut() = false;
    }
}
