use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_compression::tokio::bufread::ZstdDecoder;
use async_trait::async_trait;
use futures_util::stream::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use log::debug;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;
use wm_common::schema::event::CapturedEventRecord;

use crate::app::App;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;

pub struct CountService;

#[async_trait]
impl Service for CountService {
    fn route(&self) -> &'static str {
        "/count"
    }

    async fn serve(
        &self,
        app: Arc<App>,
        request: Request<Incoming>,
    ) -> Response<BoxBody<Bytes, hyper::Error>> {
        if request.method() == Method::POST {
            let stream = request
                .into_body()
                .into_data_stream()
                .map_err(io::Error::other);
            let mut decompressor = ZstdDecoder::new(StreamReader::new(stream));

            let mut buffer = vec![];
            if decompressor.read_to_end(&mut buffer).await.is_ok()
                && let Ok(data) = serde_json::from_str::<Vec<CapturedEventRecord>>(
                    &String::from_utf8_lossy(&buffer),
                )
            {
                debug!(
                    "Received {} uncompressed bytes of trace data ({} events)",
                    buffer.len(),
                    data.len()
                );

                let mut eps = app.eps_queue.lock().await;
                let now = Instant::now();

                eps.reserve(data.len());
                for _ in 0..data.len() {
                    eps.push_back(now);
                }

                let before = now - Duration::from_secs(1);
                while let Some(&front) = eps.front() {
                    if front < before {
                        eps.pop_front();
                    } else {
                        break;
                    }
                }

                debug!("EPS = {}", eps.len());
                return ResponseBuilder::empty(StatusCode::NO_CONTENT);
            }

            ResponseBuilder::default(StatusCode::BAD_REQUEST)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
