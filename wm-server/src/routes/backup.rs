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
use log::error;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;
use wm_common::schema::event::CapturedEventRecord;

use crate::app::App;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;
use crate::utils::parse_query_map;

pub struct BackupService;

#[async_trait]
impl Service for BackupService {
    fn route(&self) -> &'static str {
        "/backup"
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
            let decompressor = ZstdDecoder::new(StreamReader::new(stream));
            let mut chained = decompressor.chain(b"\n".as_ref());

            let mut buffer = vec![];
            while let Ok(byte) = chained.read_u8().await {
                if byte == b'\n' {
                    if buffer.is_empty() {
                        continue;
                    }

                    match serde_json::from_slice::<Vec<CapturedEventRecord>>(&buffer) {
                        Ok(events) => {
                            if !dummy {
                                match app.elastic().await {
                                    Some(elastic) => {
                                        let mut body = vec![];

                                        for event in events {
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
                                            return ResponseBuilder::default(
                                                StatusCode::SERVICE_UNAVAILABLE,
                                            );
                                        }
                                    }
                                    None => {
                                        return ResponseBuilder::default(
                                            StatusCode::SERVICE_UNAVAILABLE,
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse backup events: {e}");
                        }
                    }

                    buffer.clear();
                } else {
                    buffer.push(byte);
                }
            }

            ResponseBuilder::empty(StatusCode::NO_CONTENT)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
