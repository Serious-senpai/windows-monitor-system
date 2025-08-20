use std::sync::Arc;

use async_trait::async_trait;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use log::info;
use wm_common::schema::CapturedEventRecord;
use zstd::bulk::decompress;

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
            if let Ok(body) = request.into_body().collect().await
                && let Ok(decompressed) = decompress(&body.to_bytes(), 204800)  // 2KB should be enough for most cases, right? Haven't seen any payload larger than 1KB though.
                && let Ok(data) = serde_json::from_str::<Vec<CapturedEventRecord>>(
                    &String::from_utf8_lossy(&decompressed),
                )
            {
                info!(
                    "Received {} uncompressed bytes of trace data ({} events)",
                    decompressed.len(),
                    data.len()
                );
                return ResponseBuilder::empty(StatusCode::NO_CONTENT);
            }

            ResponseBuilder::default(StatusCode::BAD_REQUEST)
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
