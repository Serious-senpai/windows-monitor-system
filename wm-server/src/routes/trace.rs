use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_compression::tokio::bufread::ZstdDecoder;
use async_trait::async_trait;
use elasticsearch::BulkParts;
use futures_util::stream::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use log::{debug, error};
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;
use wm_common::schema::event::CapturedEventRecord;

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

                {
                    let mut eps = app.eps_queue.lock().await;
                    let now = Instant::now();
                    eps.push_back(now);

                    let before = now - Duration::from_secs(1);
                    while let Some(&front) = eps.front() {
                        if front < before {
                            eps.pop_front();
                        } else {
                            break;
                        }
                    }

                    debug!("EPS = {}", eps.len());
                }

                match app.elastic().await {
                    Some(elastic) => {
                        let mut body = vec![];

                        let create_request = "{\"create\":{}}\n".to_string();
                        for event in data {
                            body.push(create_request.clone());
                            body.push(format!("{}\n", serde_json::to_string(&event).unwrap()));
                        }

                        if let Err(e) = elastic
                            .client()
                            .bulk(BulkParts::Index(
                                "logs-endpoint.events.windows-monitor-original-test",
                            ))
                            .body(body)
                            .send()
                            .await
                        {
                            error!("{e}");
                            return ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE);
                        }

                        return ResponseBuilder::empty(StatusCode::NO_CONTENT);
                    }
                    None => {
                        return ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE);
                    }
                }
            }

            ResponseBuilder::default(StatusCode::BAD_REQUEST)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
