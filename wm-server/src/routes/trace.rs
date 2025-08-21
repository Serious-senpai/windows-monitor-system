use std::io;
use std::sync::Arc;

use async_compression::tokio::bufread::ZstdDecoder;
use async_trait::async_trait;
use futures_util::stream::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use log::info;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;
use wm_common::schema::CapturedEventRecord;

use crate::app::App;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;

pub struct TraceService;

#[async_trait]
impl Service for TraceService {
    fn route(&self) -> &'static str {
        "/trace"
    }

    async fn serve(
        &self,
        _: Arc<App>,
        request: Request<Incoming>,
    ) -> Response<BoxBody<Bytes, hyper::Error>> {
        if request.method() == Method::POST {
            let stream = request
                .into_body()
                .into_data_stream()
                .map_err(|e| io::Error::other(e));
            let mut decompressor = ZstdDecoder::new(StreamReader::new(stream));

            let mut buffer = vec![];
            if decompressor.read_to_end(&mut buffer).await.is_ok() {
                if let Ok(data) = serde_json::from_str::<Vec<CapturedEventRecord>>(
                    &String::from_utf8_lossy(&buffer),
                ) {
                    info!(
                        "Received {} uncompressed bytes of trace data ({} events)",
                        buffer.len(),
                        data.len()
                    );
                    return ResponseBuilder::empty(StatusCode::NO_CONTENT);
                }
            }

            ResponseBuilder::default(StatusCode::BAD_REQUEST)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
