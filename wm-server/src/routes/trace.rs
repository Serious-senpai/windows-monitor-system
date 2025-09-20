use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

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
use wm_common::schema::responses::TraceResponse;

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
        peer: SocketAddr,
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

                if let Some(elastic) = app.elastic().await {
                    tokio::spawn(async move {
                        let mut body = vec![];

                        debug!("Pushing {} events to Elasticsearch", data.len());
                        for event in data {
                            body.extend_from_slice(b"{\"create\":{}}\n");

                            let ecs = event.to_ecs(peer.ip());
                            serde_json::to_writer(&mut body, &ecs).unwrap();
                            body.push(b'\n');
                        }

                        if let Err(e) = elastic
                            .client()
                            .bulk(BulkParts::Index(&format!(
                                "events.windows-monitor-ecs-{}",
                                peer.ip()
                            )))
                            .body(vec![body])
                            .send()
                            .await
                        {
                            error!("Elasticsearch API error: {e}");
                        }
                    });
                }

                return ResponseBuilder::json(StatusCode::OK, TraceResponse {});
            }

            ResponseBuilder::default(StatusCode::BAD_REQUEST)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
