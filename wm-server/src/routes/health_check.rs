use std::sync::Arc;

use async_trait::async_trait;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response, StatusCode};

use crate::app::App;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;

pub struct HealthCheckService;

#[async_trait]
impl Service for HealthCheckService {
    fn route(&self) -> &'static str {
        "/health-check"
    }

    async fn serve(
        &self,
        _: Arc<App>,
        _: Request<Incoming>,
    ) -> Response<BoxBody<Bytes, hyper::Error>> {
        ResponseBuilder::empty(StatusCode::NO_CONTENT)
    }
}
