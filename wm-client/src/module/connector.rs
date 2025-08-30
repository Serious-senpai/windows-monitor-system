use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_compression::Level;
use async_compression::tokio::bufread::ZstdEncoder;
use async_trait::async_trait;
use bytes::BytesMut;
use log::{debug, error};
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex, RwLock, Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use wm_common::error::RuntimeError;
use wm_common::pool::Pool;
use wm_common::schema::event::CapturedEventRecord;

use crate::backup::Backup;
use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;

pub struct Connector {
    _configuration: Arc<Configuration>,
    _receiver: Mutex<mpsc::Receiver<Arc<CapturedEventRecord>>>,
    _running: RwLock<bool>,
    _backup: Arc<Mutex<Backup>>,

    _http: Arc<HttpClient>,
    _http_semaphore: Semaphore,

    _errors_count: Arc<RwLock<usize>>,
    _reconnect_task: JoinHandle<()>,

    _buffer_pool: Arc<Pool<BytesMut>>,
}

impl Connector {
    pub async fn async_new(
        configuration: Arc<Configuration>,
        receiver: mpsc::Receiver<Arc<CapturedEventRecord>>,
        backup: Arc<Mutex<Backup>>,
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
            _buffer_pool: Arc::new(Pool::new(concurrency_limit, |_| {
                BytesMut::with_capacity(configuration.event_post.flush_limit)
            })),
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

    /// Input must contain only the opening bracket `[` OR an incomplete JSON array with a trailing comma
    /// e.g. `[1, 2, 3,`
    async fn _send_payload_utils(self: &Arc<Self>, raw_payload: &mut Vec<u8>) {
        if raw_payload.len() == 1 {
            return;
        }

        raw_payload.pop(); // Remove trailing comma
        raw_payload.push(b']');

        let mut write_to_backup = self._disconnected().await;
        if !write_to_backup {
            let mut compressor = ZstdEncoder::with_quality(
                raw_payload.as_slice(),
                Level::Precise(self._configuration.zstd_compression_level),
            );
            let mut compressed = self._buffer_pool.acquire().await;
            compressed.clear();

            if let Err(e) = compressor.read(&mut **compressed).await {
                error!("Unable to compress data: {e}");
            } else {
                debug!(
                    "Sending {} bytes of uncompressed data (compressed to {} bytes)",
                    raw_payload.len(),
                    compressed.len()
                );

                if let Ok(_) = self._http_semaphore.acquire().await {
                    // Connection state may have been updated while waiting for the semaphore
                    if self._disconnected().await {
                        write_to_backup = true;
                    } else if let Err(e) = self
                        ._http
                        .api()
                        .post("/count")
                        .body(compressed.clone().freeze())
                        .send()
                        .await
                    {
                        error!(
                            "Failed to send trace event to server: {e:?}, writing to backup instead"
                        );

                        let mut errors_count = self._errors_count.write().await;
                        *errors_count = (*errors_count + 1)
                            .min(self._configuration.event_post.concurrency_limit);
                        write_to_backup = true;
                    }
                }
            }
        }

        if write_to_backup {
            debug!(
                "Backing up {} bytes of uncompressed data",
                raw_payload.len()
            );
            let mut backup = self._backup.lock().await;

            backup.write_raw(raw_payload).await;
            backup.write_raw(b"\n").await;
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

        let mut raw_payload = Vec::with_capacity(self._configuration.event_post.flush_limit);
        raw_payload.push(b'[');

        let mut receiver = self._receiver.lock().await;
        loop {
            if !*self._running.read().await {
                break;
            }

            match timeout(Duration::from_secs(1), receiver.recv()).await {
                Ok(Some(event)) => {
                    if let Err(e) = serde_json::to_writer(&mut raw_payload, &event) {
                        error!("Failed to serialize {event:?}: {e}");
                        raw_payload.clear();
                        raw_payload.push(b'[');
                    } else {
                        raw_payload.push(b',');
                        if raw_payload.len() > self._configuration.event_post.flush_limit {
                            self._send_payload_utils(&mut raw_payload).await;
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    self._send_payload_utils(&mut raw_payload).await;
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
