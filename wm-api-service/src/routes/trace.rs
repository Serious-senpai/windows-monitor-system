use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_compression::tokio::bufread::ZstdDecoder;
use async_trait::async_trait;
use futures_util::stream::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use lapin::BasicProperties;
use lapin::options::BasicPublishOptions;
use log::error;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;
use wm_common::schema::responses::TraceResponse;

use crate::app::App;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;
use crate::utils::append_client_ip;

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
            let decompressor = ZstdDecoder::new(StreamReader::new(stream));
            let mut chained = decompressor.chain(b"\n".as_ref());

            tokio::spawn(async move {
                match app.rabbitmq().await {
                    Some(rabbitmq) => {
                        let mut buffer = vec![];
                        let options = BasicPublishOptions::default();
                        let properties = BasicProperties::default();
                        while let Ok(byte) = chained.read_u8().await {
                            if byte == b'\n' {
                                if buffer.is_empty() {
                                    continue;
                                }

                                append_client_ip(&mut buffer, peer.ip());

                                if let Err(e) = rabbitmq
                                    .basic_publish(
                                        "",
                                        "events",
                                        options,
                                        &buffer,
                                        properties.clone(),
                                    )
                                    .await
                                {
                                    error!("RabbitMQ error: {e}");
                                }

                                buffer.clear();
                            } else {
                                buffer.push(byte);
                            }
                        }
                    }
                    None => {
                        error!("RabbitMQ connection is not available. Events are lost from {peer}");
                    }
                }
            });

            ResponseBuilder::json(StatusCode::OK, TraceResponse {})
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
