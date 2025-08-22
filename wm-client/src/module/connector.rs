use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, error};
use tokio::fs;
use tokio::sync::{Mutex, RwLock, Semaphore, mpsc};
use tokio::time::timeout;
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
        Self {
            _configuration: configuration.clone(),
            _receiver: Mutex::new(receiver),
            _running: RwLock::new(false),
            _backup: backup,
            _http: http,
            _http_semaphore: Semaphore::new(configuration.event_post.concurrency_limit),
        }
    }

    async fn _send_payload_utils(self: Arc<Self>, raw_payload: &mut Vec<u8>) {
        if raw_payload.len() == 1 {
            return;
        }

        raw_payload.pop(); // Remove trailing comma
        raw_payload.push(b']');

        debug!("Sending {} bytes of uncompressed data", raw_payload.len());
        // TODO: Implement compression, HTTP request and error handling

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
