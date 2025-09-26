use std::io;
use std::net::{IpAddr, SocketAddr};
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

use crate::app::App;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;

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
            let stream = request
                .into_body()
                .into_data_stream()
                .map_err(io::Error::other);
            let decompressor = ZstdDecoder::new(StreamReader::new(stream));
            let mut chained = decompressor.chain(b"\n".as_ref());

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

                            let ip_native_order = match peer.ip() {
                                IpAddr::V4(ipv4) => u128::from(ipv4.to_bits()),
                                IpAddr::V6(ipv6) => ipv6.to_bits(),
                            };
                            buffer.extend_from_slice(&ip_native_order.to_be_bytes());
                            buffer.push(matches!(peer.ip(), IpAddr::V4(_)) as u8);

                            if let Err(e) = rabbitmq
                                .basic_publish("", "events", options, &buffer, properties.clone())
                                .await
                            {
                                error!("RabbitMQ error: {e}");
                                return ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE);
                            }

                            buffer.clear();
                        } else {
                            buffer.push(byte);
                        }
                    }
                }
                None => {
                    return ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE);
                }
            }

            ResponseBuilder::empty(StatusCode::NO_CONTENT)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
