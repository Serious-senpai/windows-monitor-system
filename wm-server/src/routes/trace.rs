use async_trait::async_trait;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response};
use log::info;
use wm_common::schema::CapturedEventRecord;

use crate::routes::abc::Service;
use crate::utils;

pub struct TraceService;

#[async_trait]
impl Service for TraceService {
    fn route(&self) -> &'static str {
        "/trace"
    }

    async fn serve(&self, request: Request<Incoming>) -> Response<BoxBody<Bytes, hyper::Error>> {
        if request.method() == Method::POST {
            if let Ok(body) = request.into_body().collect().await
                && let Ok(data) = serde_json::from_str::<CapturedEventRecord>(
                    &String::from_utf8_lossy(&body.to_bytes()),
                )
            {
                info!("Received {data:?}");
                return utils::ok_no_content();
            }

            utils::bad_request()
        } else {
            utils::method_not_allowed()
        }
    }
}
