pub mod providers;

use std::collections::VecDeque;
use std::error::Error;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_compression::Level;
use async_compression::tokio::bufread::ZstdEncoder;
use async_trait::async_trait;
use bytes::Bytes;
use ferrisetw::trace::{KernelTrace, TraceBuilder, TraceTrait};
use log::{debug, error};
use reqwest::Body;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, RwLock, Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio::{fs, task};
use tokio_util::io::ReaderStream;
use wm_common::error::RuntimeError;
use wm_common::schema::{CapturedEventRecord, Event};
use wm_common::sysinfo::SystemInfo;

use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;
use crate::module::tracer::providers::ProviderWrapper;
use crate::module::tracer::providers::file::FileProviderWrapper;
use crate::module::tracer::providers::image::ImageProviderWrapper;
use crate::module::tracer::providers::process::ProcessProviderWrapper;
use crate::module::tracer::providers::tcpip::TcpIpProviderWrapper;
use crate::module::tracer::providers::udpip::UdpIpProviderWrapper;

pub struct EventTracer {
    _configuration: Arc<Configuration>,
    _http: Arc<HttpClient>,
    _trace: RwLock<Option<KernelTrace>>,
    _running: Mutex<bool>,
    _http_semaphore: Semaphore,
    _backup: Mutex<fs::File>,
    _send_previous_backup: JoinHandle<()>,
}

impl EventTracer {
    pub async fn new(configuration: Arc<Configuration>, http: Arc<HttpClient>) -> Self
    where
        Self: Sized,
    {
        let _ = fs::create_dir_all(&configuration.backup_directory).await;

        let mut index = 0;
        while Self::_get_log_file_path(configuration.clone(), index).exists() {
            index += 1;
            if index == 1000 {
                panic!("Too many backup files");
            }
        }

        let backup_path = Self::_get_log_file_path(configuration.clone(), index);
        let backup = fs::File::create(&backup_path)
            .await
            .expect("Failed to create backup file");

        let configuration_cloned = configuration.clone();
        let http_cloned = http.clone();
        let send_previous_backup = tokio::spawn(async move {
            Self::_send_previous_backup_impl(configuration_cloned, http_cloned, backup_path).await;
        });

        Self {
            _configuration: configuration,
            _http: http,
            _trace: RwLock::new(None),
            _running: Mutex::new(false),
            _http_semaphore: Semaphore::new(5),
            _backup: Mutex::new(backup),
            _send_previous_backup: send_previous_backup,
        }
    }

    async fn _send_previous_backup_impl(
        configuration: Arc<Configuration>,
        http: Arc<HttpClient>,
        exclude: PathBuf,
    ) {
        match fs::read_dir(&configuration.backup_directory).await {
            Ok(mut reader) => {
                while let Ok(Some(entry)) = reader.next_entry().await {
                    let path = entry.path();
                    if path == exclude {
                        continue;
                    }

                    match OpenOptions::new().read(true).open(&path).await {
                        Ok(file) => {
                            let compressor = ZstdEncoder::with_quality(
                                BufReader::new(file),
                                Level::Precise(configuration.zstd_compression_level),
                            );
                            if let Err(e) = http
                                .api()
                                .post("/backup")
                                .body(Body::wrap_stream(ReaderStream::new(compressor)))
                                .send()
                                .await
                            {
                                error!("Failed to send backup {} to server: {e:?}", path.display());
                            } else {
                                let _ = fs::remove_file(&path).await;
                            }
                        }
                        Err(e) => {
                            error!("Failed to read backup file {}: {e}", path.display());
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to read backup directory: {e}");
            }
        }
    }

    fn _get_log_file_path(configuration: Arc<Configuration>, index: i32) -> PathBuf {
        configuration
            .backup_directory
            .join(format!("backup-{index}.jsonl"))
    }

    fn _trace_builder(sender: mpsc::UnboundedSender<Event>) -> TraceBuilder<KernelTrace> {
        let mut tracer = KernelTrace::new().named("Windows Monitor Event Tracer".into());
        let wrappers: Vec<Arc<dyn ProviderWrapper>> = vec![
            Arc::new(FileProviderWrapper::new()),
            Arc::new(ImageProviderWrapper::new()),
            Arc::new(ProcessProviderWrapper::new()),
            Arc::new(TcpIpProviderWrapper::new()),
            Arc::new(UdpIpProviderWrapper::new()),
            // Add other providers here as needed
        ];
        for wrapper in wrappers {
            tracer = wrapper.attach(tracer, sender.clone());
        }

        tracer
    }

    async fn _set_trace(&self, trace: KernelTrace) {
        let mut self_trace = self._trace.write().await;
        *self_trace = Some(trace);
    }

    async fn _send(
        self: Arc<Self>,
        events: &mut VecDeque<CapturedEventRecord>,
    ) -> Option<JoinHandle<()>> {
        let events_to_send = events.drain(..).collect::<Vec<CapturedEventRecord>>();
        match serde_json::to_vec(&events_to_send) {
            Ok(data) => {
                Some(tokio::spawn(async move {
                    let data = Bytes::from(data);
                    let reader = BufReader::new(Cursor::new(data.clone()));
                    let compressor = ZstdEncoder::with_quality(
                        reader,
                        Level::Precise(self._configuration.zstd_compression_level),
                    );

                    #[allow(clippy::redundant_pattern_matching)]
                    if let Ok(_) = self._http_semaphore.acquire().await {
                        // Using `.is_ok()` will immediately release the semaphore
                        if let Err(e) = self
                            ._http
                            .api()
                            .post("/trace")
                            .body(Body::wrap_stream(ReaderStream::new(compressor)))
                            .send()
                            .await
                        {
                            error!(
                                "Failed to send trace event to server: {e:?}, writing to backup instead"
                            );

                            let mut backup = self._backup.lock().await;

                            if let Err(e) = backup.write(&data).await {
                                error!("Failed to backup data: {e}");
                            }
                            let _ = backup.write(b"\n").await;
                        }
                    }
                }))
            }
            Err(e) => {
                error!("Unable to serialize events: {e}");
                None
            }
        }
    }

    async fn _poll_and_send(
        self: Arc<Self>,
        mut receiver: mpsc::UnboundedReceiver<Event>,
    ) -> (mpsc::UnboundedReceiver<Event>, Option<JoinHandle<()>>) {
        let mut events = VecDeque::new();
        let mut last_task = None;
        loop {
            match timeout(Duration::from_secs(1), receiver.recv()).await {
                Ok(Some(event)) => {
                    events.push_back(CapturedEventRecord {
                        event,
                        system: SystemInfo::fetch().await,
                    });

                    if events.len() >= self._configuration.events_per_request {
                        last_task = self.clone()._send(&mut events).await;
                    }
                }
                Ok(None) => break (receiver, last_task),
                Err(_) => {
                    if !events.is_empty() {
                        last_task = self.clone()._send(&mut events).await;
                    }
                }
            }
        }
    }
}

#[async_trait]
impl Module for EventTracer {
    fn name(&self) -> &str {
        "EventTracer"
    }

    async fn run(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        {
            let mut running = self._running.lock().await;
            if *running {
                return Err(RuntimeError::new("EventTracer is already running").into());
            }

            *running = true;
        }

        debug!("Running EventTracer");

        let (sender, mut receiver) = mpsc::unbounded_channel();
        let process_handle_self = self.clone();
        let process_handle =
            tokio::spawn(async move { process_handle_self.clone()._poll_and_send(receiver).await });

        let (trace, handle) = Self::_trace_builder(sender)
            .start()
            .map_err(|e| RuntimeError::new(format!("Unable to start kernel trace: {e:?}")))?;

        self._set_trace(trace).await;

        // Process trace in a blocking thread; this call will block until the trace stops.
        task::spawn_blocking(move || KernelTrace::process_from_handle(handle))
            .await?
            .map_err(|e| RuntimeError::new(format!("Kernel trace error: {e:?}")))?;

        let process_result = process_handle
            .await
            .map_err(|e| RuntimeError::new(format!("Unable to reobtain receiver: {e:?}")))?;

        receiver = process_result.0;
        receiver.close();

        if let Some(task) = process_result.1 {
            let _ = task.await;
        }

        debug!("EventTracer completed");

        Ok(())
    }

    async fn stop(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut running = self._running.lock().await;
        if !*running {
            return Err(RuntimeError::new("EventTracer is not running").into());
        }

        debug!("Stopping EventTracer");

        {
            let mut self_trace = self._trace.write().await;
            if let Some(trace) = self_trace.take() {
                trace
                    .stop()
                    .map_err(|e| RuntimeError::new(format!("Error stopping EventTracer: {e:?}")))?;
            }
        }

        *running = false;
        self._http_semaphore.close();
        self._send_previous_backup.abort();
        debug!("EventTracer stopped");

        Ok(())
    }
}

impl Drop for EventTracer {
    fn drop(&mut self) {
        if let Some(trace) = self._trace.get_mut().take()
            && let Err(e) = trace.stop()
        {
            error!("Error stopping EventTracer: {e:?}");
        }

        *self._running.get_mut() = false;
    }
}
