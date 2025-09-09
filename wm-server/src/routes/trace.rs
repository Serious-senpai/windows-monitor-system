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
use crate::eps::EPSQueue;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;
use crate::utils::parse_query_map;

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
            let query = parse_query_map(&request);
            let dummy = query.contains_key("dummy");

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

                let (emit_eps, receive_eps) = {
                    let mut map = app.eps_queue().lock().await;
                    let queue = map.entry(peer.ip()).or_insert_with(EPSQueue::new);
                    queue.count_eps(&data);

                    (queue.emit_eps(), queue.receive_eps())
                };
                if !dummy {
                    match app.elastic().await {
                        Some(elastic) => {
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
                                error!("{e}");
                                return ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE);
                            }
                        }
                        None => {
                            return ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE);
                        }
                    }
                }

                return ResponseBuilder::json(
                    StatusCode::OK,
                    TraceResponse {
                        emit_eps,
                        receive_eps,
                    },
                );
            }

            ResponseBuilder::default(StatusCode::BAD_REQUEST)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
