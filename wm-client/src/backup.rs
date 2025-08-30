use async_compression::tokio::write::ZstdEncoder;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use wm_common::schema::event::CapturedEventRecord;

pub struct Backup {
    _zstd: ZstdEncoder<BufWriter<fs::File>>,
}

impl Backup {
    pub fn new(file: fs::File) -> Self {
        Self {
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

    pub async fn write_raw(&mut self, data: &[u8]) {
        self._zstd.write_all(data).await.unwrap();
    }

    pub async fn flush(&mut self) {
        self._zstd.flush().await.unwrap();
        self._zstd.get_mut().flush().await.unwrap();
    }
}
