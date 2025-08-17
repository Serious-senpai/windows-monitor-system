use async_trait::async_trait;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};

#[async_trait]
pub trait Service: Send + Sync {
    fn route(&self) -> &'static str;
    async fn serve(&self, request: Request<Incoming>) -> Response<BoxBody<Bytes, hyper::Error>>;
}
