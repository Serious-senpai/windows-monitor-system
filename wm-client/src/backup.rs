use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_compression::tokio::write::ZstdEncoder;
use log::{error, info, warn};
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{Mutex, SetOnce};
use wm_common::file;
use wm_common::schema::event::CapturedEventRecord;

use crate::http::HttpClient;

pub struct Backup {
    _backup_directory: PathBuf,
    _path: PathBuf,
    _zstd: ZstdEncoder<BufWriter<fs::File>>,
}

impl Backup {
    fn _get_log_file_path(backup_directory: &Path, index: i32) -> PathBuf {
        backup_directory.join(format!("backup-{index}.zst"))
    }

    async fn _switch_to_new_path(
        backup_directory: &Path,
    ) -> (PathBuf, ZstdEncoder<BufWriter<fs::File>>) {
        let _ = fs::create_dir_all(backup_directory).await;
        let mut index = 0;
        let (file, mut path) = loop {
            let backup_path = Self::_get_log_file_path(backup_directory, index);
            match file::create_new_exclusively(&backup_path) {
                Ok(f) => break (f, backup_path),
                Err(_) => {
                    index += 1;
                    if index == 1000 {
                        panic!("Failed to create a new backup file after 1000 attempts");
                    }
                }
            }
        };

        path = path.canonicalize().unwrap_or(path);
        info!("Switched to backup file: {}", path.display());
        (path, ZstdEncoder::new(BufWriter::new(file)))
    }

    pub async fn async_new(backup_directory: PathBuf) -> Self {
        let (path, zstd) = Self::_switch_to_new_path(&backup_directory).await;

        Self {
            _backup_directory: backup_directory,
            _path: path,
            _zstd: zstd,
        }
    }

    pub fn path(&self) -> &Path {
        &self._path
    }

    pub async fn switch_backup(&mut self) {
        self.flush().await;

        let (path, zstd) = Self::_switch_to_new_path(&self._backup_directory).await;
        self._path = path;
        self._zstd = zstd;
    }

    pub async fn write_one(&mut self, data: &CapturedEventRecord) {
        self._zstd
            .write_all(&data.serialize_to_vec())
            .await
            .unwrap();
        self._zstd.write_u8(b'\n').await.unwrap();
    }

    pub async fn write_many(&mut self, data: &[CapturedEventRecord]) {
        for record in data {
            self.write_one(record).await;
        }
    }

    pub async fn write(&mut self, data: &[u8]) {
        self._zstd.write_all(data).await.unwrap();
    }

    pub async fn flush(&mut self) {
        self._zstd.flush().await.unwrap();
        self._zstd.get_mut().flush().await.unwrap();
    }

    pub async fn upload(
        backup: Arc<Mutex<Self>>,
        http: Arc<HttpClient>,
        stopped: Arc<SetOnce<()>>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let backup_directory = backup.lock().await._backup_directory.clone();

        let mut entries = fs::read_dir(&backup_directory).await?;
        while let Ok(Some(entry)) = entries.next_entry().await
            && stopped.get().is_none()
        {
            if entry.path().extension().is_none_or(|s| s != "zst")
                || entry.path() == backup.lock().await._path
            {
                continue;
            }

            info!("Sending backup {}", entry.path().display());

            match file::open_exclusively(entry.path()) {
                Ok(file) => match http.api().post("/backup").body(file).send().await {
                    Ok(response) => {
                        if response.status() == 204 {
                            info!("Uploaded backup {}", entry.path().display());
                            if let Err(e) = fs::remove_file(entry.path()).await {
                                error!(
                                    "Failed to delete backup {} after upload: {e}",
                                    entry.path().display()
                                );
                            }
                        } else {
                            error!(
                                "Backup response {} for {}",
                                response.status(),
                                entry.path().display()
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to send backup {} to server: {e}",
                            entry.path().display()
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        "Unable to open backup {} for reading. Skipping: {e}",
                        entry.path().display()
                    );
                }
            }
        }

        Ok(())
    }
}
