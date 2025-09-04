use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use async_compression::tokio::write::ZstdEncoder;
use log::{debug, error, info};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::task::JoinHandle;
use wm_common::schema::event::CapturedEventRecord;

use crate::configuration::Configuration;
use crate::http::HttpClient;

pub struct Backup {
    _config: Arc<Configuration>,
    _path: PathBuf,
    _zstd: ZstdEncoder<BufWriter<fs::File>>,
}

impl Backup {
    fn _get_log_file_path(config: Arc<Configuration>, index: i32) -> PathBuf {
        config.backup_directory.join(format!("backup-{index}.zst"))
    }

    pub async fn async_new(config: Arc<Configuration>) -> Self {
        let _ = fs::create_dir_all(&config.backup_directory).await;

        let mut index = 0;
        let (file, path) = loop {
            let backup_path = Self::_get_log_file_path(config.clone(), index);
            match fs::File::create_new(&backup_path).await {
                Ok(f) => break (f, backup_path),
                Err(_) => {
                    index += 1;
                    if index == 1000 {
                        panic!("Failed to create a new backup file after 1000 attempts");
                    }
                }
            }
        };

        Self {
            _config: config,
            _path: path,
            _zstd: ZstdEncoder::new(BufWriter::new(file)),
        }
    }

    pub async fn write_one(&mut self, data: &CapturedEventRecord) {
        self._zstd.write_u8(b'[').await.unwrap();
        self._zstd
            .write_all(&serde_json::to_vec(data).unwrap())
            .await
            .unwrap();
        self._zstd.write_all(b"]\n").await.unwrap();
    }

    pub async fn write_many(&mut self, data: &[CapturedEventRecord]) {
        self._zstd
            .write_all(&serde_json::to_vec(data).unwrap())
            .await
            .unwrap();
        self._zstd.write_u8(b'\n').await.unwrap();
    }

    pub async fn write_raw<const COMPRESSED: bool>(&mut self, data: &[u8]) {
        if COMPRESSED {
            self._zstd.write_all(data).await.unwrap();
        } else {
            self._zstd.get_mut().write_all(data).await.unwrap();
        }
    }

    pub async fn flush(&mut self) {
        self._zstd.flush().await.unwrap();
        self._zstd.get_mut().flush().await.unwrap();
    }

    pub fn upload(
        &self,
        http: Arc<HttpClient>,
    ) -> JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
        let backup_directory = self._config.backup_directory.clone();
        let ignore = self._path.clone();
        tokio::spawn(async move {
            let mut entries = fs::read_dir(&backup_directory).await?;
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path() == ignore {
                    continue;
                }

                debug!("Sending backup {}", entry.path().display());

                let mut file = fs::File::open(entry.path()).await?;
                let mut buffer = vec![];
                if let Ok(metadata) = file.metadata().await {
                    buffer.reserve(metadata.len() as usize);
                }

                file.read_buf(&mut buffer).await?;

                match http.api().post("/backup?dummy").body(buffer).send().await {
                    Ok(response) => {
                        if response.status() == 204 {
                            info!("Uploaded backup {}", entry.path().display());
                            if let Err(e) = fs::remove_file(entry.path()).await {
                                error!("Failed to delete backup file after upload: {e}");
                            }
                        } else {
                            error!("Backup response {}", response.status());
                        }
                    }
                    Err(e) => {
                        error!("Failed to send backup to server: {e}");
                    }
                }
            }

            Ok(())
        })
    }
}
