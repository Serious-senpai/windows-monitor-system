use std::error::Error;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_compression::Level;
use async_compression::tokio::bufread::ZstdEncoder;
use async_trait::async_trait;
use bytes::BytesMut;
use log::{debug, error};
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock, SetOnce, mpsc};
use tokio::task::JoinHandle;
use tokio::time::error::Elapsed;
use tokio::time::{sleep, timeout};
use wm_common::pool::Pool;
use wm_common::schema::event::CapturedEventRecord;
use wm_common::schema::responses::TraceResponse;

use crate::backup::Backup;
use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;

pub struct Connector {
    _config: Arc<Configuration>,
    _receiver: Mutex<mpsc::Receiver<Arc<CapturedEventRecord>>>,
    _stopped: Arc<SetOnce<()>>,
    _backup: Arc<Mutex<Backup>>,

    _http: Arc<HttpClient>,

    _errors_count: Arc<RwLock<usize>>,
    _reconnect: Arc<Reconnector>,
    _reconnect_task: Mutex<Option<JoinHandle<()>>>,

    _uncompressed_buffer_pool: Vec<Arc<Mutex<Vec<u8>>>>,
    _uncompressed_buffer_pool_index: AtomicUsize,
    _compressed_buffer_pool: Arc<Pool<BytesMut>>,
}

impl Connector {
    pub fn new(
        configuration: Arc<Configuration>,
        receiver: mpsc::Receiver<Arc<CapturedEventRecord>>,
        backup: Arc<Mutex<Backup>>,
        http: Arc<HttpClient>,
    ) -> Arc<Self>
    where
        Self: Sized,
    {
        let concurrency_limit = configuration.event_post.concurrency_limit;
        let errors_count = Arc::new(RwLock::new(0));

        let mut uncompressed_buffer_pool = vec![];
        for _ in 0..configuration.event_post.concurrency_limit {
            let mut buffer = Vec::with_capacity(configuration.event_post.flush_limit * 3 / 2);
            buffer.push(b'[');

            let payload = Arc::new(Mutex::new(buffer));
            uncompressed_buffer_pool.push(payload);
        }

        Arc::new_cyclic(|weak| Self {
            _config: configuration.clone(),
            _receiver: Mutex::new(receiver),
            _stopped: Arc::new(SetOnce::new()),
            _backup: backup,
            _http: http,
            _errors_count: errors_count,
            _reconnect: Arc::new(Reconnector::new(weak.clone())),
            _reconnect_task: Mutex::new(None),
            _uncompressed_buffer_pool: uncompressed_buffer_pool,
            _uncompressed_buffer_pool_index: AtomicUsize::new(0),
            _compressed_buffer_pool: Arc::new(Pool::new(concurrency_limit, |_| {
                BytesMut::with_capacity(8192) // these buffers are for compressed data, so we cannot predict them anyway (let's start with 8KB!)
            })),
        })
    }

    async fn _disconnected(&self) -> bool {
        *self._errors_count.read().await == self._config.event_post.concurrency_limit
    }

    /// Input must contain only the opening bracket `[` OR an incomplete JSON array with a trailing comma
    /// e.g. `[1, 2, 3,`
    async fn _send_payload_utils(self: &Arc<Self>, mut raw_payload: OwnedMutexGuard<Vec<u8>>) {
        if raw_payload.len() == 1 {
            return;
        }

        raw_payload.pop(); // Remove trailing comma
        raw_payload.push(b']');

        let mut write_to_backup = self._disconnected().await;
        if !write_to_backup {
            let mut compressor = ZstdEncoder::with_quality(
                raw_payload.as_slice(),
                Level::Precise(self._config.zstd_compression_level),
            );
            let mut buffer = self._compressed_buffer_pool.acquire().await;
            buffer.clear();

            let success = if let Err(e) = compressor.read_buf(&mut *buffer).await {
                error!("Unable to compress data: {e}");
                false
            } else {
                debug!(
                    "Sending {} bytes of uncompressed data (compressed to {} bytes)",
                    raw_payload.len(),
                    buffer.len(),
                );

                match self
                    ._http
                    .api()
                    .post("/trace")
                    .body(buffer.clone().freeze())
                    .send()
                    .await
                {
                    Ok(response) => {
                        response.status() == 200
                            && match response.json::<TraceResponse>().await {
                                Ok(data) => {
                                    debug!("Server response {data:?}");
                                    true
                                }
                                Err(e) => {
                                    error!("Invalid server JSON response: {e}");
                                    false
                                }
                            }
                    }
                    Err(e) => {
                        error!(
                            "Failed to send trace event to server: {e}, writing to backup instead"
                        );
                        false
                    }
                }
            };

            if !success {
                let mut errors_count = self._errors_count.write().await;
                *errors_count = (*errors_count + 1).min(self._config.event_post.concurrency_limit);
                write_to_backup = true;
            }
        }

        if write_to_backup {
            // Sadly we cannot reuse the compressed buffer above because the backup stream maintains its own state
            debug!(
                "Backing up {} bytes of uncompressed data",
                raw_payload.len(),
            );

            let mut backup = self._backup.lock().await;
            backup.write(raw_payload.as_slice()).await;
            backup.write(b"\n").await;
        }

        raw_payload.clear();
        raw_payload.push(b'[');
    }
}

#[async_trait]
impl Module for Connector {
    type EventType = Result<Option<Arc<CapturedEventRecord>>, Elapsed>;

    fn name(&self) -> &str {
        "Connector"
    }

    fn stopped(&self) -> Arc<SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        let mut receiver = self._receiver.lock().await;
        timeout(Duration::from_secs(1), receiver.recv()).await
    }

    async fn before_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let reconnect = self._reconnect.clone();
        let reconnect_task = tokio::spawn(async move {
            let _ = reconnect.clone().run().await;
        });
        self._reconnect_task.lock().await.replace(reconnect_task);
        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        self._reconnect.stop();
        if let Some(reconnect_task) = self._reconnect_task.lock().await.take() {
            reconnect_task.await?;
        }

        // Flush any remaining data in the buffers
        let mut tasks = vec![];
        for payload in &self._uncompressed_buffer_pool {
            let payload = payload.clone().lock_owned().await;
            let ptr = self.clone();
            tasks.push(tokio::spawn(async move {
                ptr._send_payload_utils(payload).await
            }));
        }

        for task in tasks {
            task.await?;
        }

        Ok(())
    }

    async fn handle(
        self: Arc<Self>,
        event: Self::EventType,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Ordering::Relaxed is sufficient because `.handle()` calls never overlap
        let index = self._uncompressed_buffer_pool_index.load(Ordering::Relaxed);
        let mut payload = self._uncompressed_buffer_pool[index]
            .clone()
            .lock_owned()
            .await;

        let ptr = self.clone();
        match event {
            Ok(Some(event)) => {
                if let Err(e) = event.serialize_to_writer(&mut *payload) {
                    error!("Failed to serialize {event:?}: {e}");
                    payload.clear();
                    payload.push(b'[');
                } else {
                    payload.push(b',');
                    if payload.len() > self._config.event_post.flush_limit {
                        tokio::spawn(async move { ptr._send_payload_utils(payload).await });
                        self._uncompressed_buffer_pool_index.store(
                            (index + 1) % self._uncompressed_buffer_pool.len(),
                            Ordering::Relaxed,
                        );
                    }
                }
            }
            Ok(None) => {}
            Err(_) => {
                tokio::spawn(async move { ptr._send_payload_utils(payload).await });
                self._uncompressed_buffer_pool_index.store(
                    (index + 1) % self._uncompressed_buffer_pool.len(),
                    Ordering::Relaxed,
                );
            }
        }

        Ok(())
    }
}

struct Reconnector {
    _parent: Weak<Connector>,
    _stopped: Arc<SetOnce<()>>,
    _sleep_secs: AtomicU64,
}

impl Reconnector {
    pub fn new(parent: Weak<Connector>) -> Self {
        Self {
            _parent: parent,
            _stopped: Arc::new(SetOnce::new()),
            _sleep_secs: AtomicU64::new(5),
        }
    }
}

#[async_trait]
impl Module for Reconnector {
    type EventType = ();

    fn name(&self) -> &str {
        "Reconnector"
    }

    fn stopped(&self) -> Arc<SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        sleep(Duration::from_secs(
            self._sleep_secs.load(Ordering::Relaxed),
        ))
        .await;
    }

    async fn handle(
        self: Arc<Self>,
        _: Self::EventType,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Ordering::Relaxed is sufficient because `.handle()` and `.listen()` calls never overlap
        let parent = match self._parent.upgrade() {
            Some(parent) => parent,
            None => return Ok(()),
        };

        if parent._disconnected().await {
            debug!("Attempting to reconnect to server...");
            if let Ok(response) = parent._http.api().get("/health-check").send().await
                && response.status() == 204
            {
                *parent._errors_count.write().await = 0;
                self._sleep_secs.store(5, Ordering::Relaxed);
            } else {
                let _ = self
                    ._sleep_secs
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                        Some((v * 3 / 2).min(60))
                    });
            }
        }

        Ok(())
    }
}
