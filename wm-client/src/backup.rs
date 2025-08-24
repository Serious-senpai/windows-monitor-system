use std::io;

use async_compression::tokio::write::ZstdEncoder;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub struct Backup {
    _zstd: ZstdEncoder<fs::File>,
}

impl Backup {
    pub fn new(file: fs::File) -> Self {
        Self {
            _zstd: ZstdEncoder::new(file),
        }
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), io::Error> {
        self._zstd.write_all(data).await
    }
}
