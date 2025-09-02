use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Bytes;
use hyper::{Response, StatusCode};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct _DefaultResponse {
    pub error: bool,
    pub message: String,
}

pub struct ResponseBuilder;

impl ResponseBuilder {
    pub fn json<T>(status: StatusCode, body: T) -> Response<BoxBody<Bytes, hyper::Error>>
    where
        T: Serialize,
    {
        Response::builder()
            .status(status)
            .body(BoxBody::new(
                Full::from(
                    serde_json::to_string(&body).expect("JSON serialization should never fail"),
                )
                .map_err(|_| unreachable!()),
            ))
            .unwrap()
    }

    pub fn empty(status: StatusCode) -> Response<BoxBody<Bytes, hyper::Error>> {
        Response::builder()
            .status(status)
            .body(BoxBody::new(Empty::new().map_err(|_| unreachable!())))
            .unwrap()
    }

    pub fn message<S>(status: StatusCode, message: S) -> Response<BoxBody<Bytes, hyper::Error>>
    where
        S: Into<String>,
    {
        Self::json(
            status,
            _DefaultResponse {
                error: status.is_client_error() || status.is_server_error(),
                message: message.into(),
            },
        )
    }

    pub fn default(status: StatusCode) -> Response<BoxBody<Bytes, hyper::Error>> {
        Self::message(
            status,
            status.canonical_reason().unwrap_or("Unknown status"),
        )
    }
}
