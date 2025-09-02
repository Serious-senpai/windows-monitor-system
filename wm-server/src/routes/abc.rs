use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};

use crate::app::App;

#[async_trait]
pub trait Service: Send + Sync {
    fn route(&self) -> &'static str;
    async fn serve(
        &self,
        app: Arc<App>,
        peer: SocketAddr,
        request: Request<Incoming>,
    ) -> Response<BoxBody<Bytes, hyper::Error>>;
}
