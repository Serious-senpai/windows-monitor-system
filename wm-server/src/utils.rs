use std::str;
use std::sync::LazyLock;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::Response;
use hyper::body::Bytes;
use openssl::base64::{decode_block, encode_block};
use openssl::sha::sha512;
use rand::Rng;

static _BAD_REQUEST: LazyLock<Full<Bytes>> = LazyLock::new(|| Full::from("Bad request"));
static _FORBIDDEN: LazyLock<Full<Bytes>> = LazyLock::new(|| Full::from("Forbidden"));
static _NOT_FOUND: LazyLock<Full<Bytes>> = LazyLock::new(|| Full::from("Not found"));
static _METHOD_NOT_ALLOWED: LazyLock<Full<Bytes>> =
    LazyLock::new(|| Full::from("Method not allowed"));
static _INTERNAL_SERVER_ERROR: LazyLock<Full<Bytes>> =
    LazyLock::new(|| Full::from("Internal server error"));
static _SERVICE_UNAVAILABLE: LazyLock<Full<Bytes>> =
    LazyLock::new(|| Full::from("Service unavailable"));

pub fn ok_str<T>(body: T) -> Response<BoxBody<Bytes, hyper::Error>>
where
    T: Into<String>,
{
    Response::builder()
        .status(200)
        .body(BoxBody::new(
            Full::from(body.into()).map_err(|_| unreachable!()),
        ))
        .unwrap()
}

pub fn ok_no_content() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(204)
        .body(BoxBody::new(
            Full::from(Bytes::new()).map_err(|_| unreachable!()),
        ))
        .unwrap()
}

pub fn bad_request() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(400)
        .body(BoxBody::new(
            _BAD_REQUEST.clone().map_err(|_| unreachable!()),
        ))
        .unwrap()
}

pub fn forbidden() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(403)
        .body(BoxBody::new(_FORBIDDEN.clone().map_err(|_| unreachable!())))
        .unwrap()
}

pub fn not_found() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(404)
        .body(BoxBody::new(_NOT_FOUND.clone().map_err(|_| unreachable!())))
        .unwrap()
}

pub fn method_not_allowed() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(405)
        .body(BoxBody::new(
            _METHOD_NOT_ALLOWED.clone().map_err(|_| unreachable!()),
        ))
        .unwrap()
}

pub fn internal_server_error() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(500)
        .body(BoxBody::new(
            _INTERNAL_SERVER_ERROR.clone().map_err(|_| unreachable!()),
        ))
        .unwrap()
}

pub fn service_unavailable() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(503)
        .body(BoxBody::new(
            _SERVICE_UNAVAILABLE.clone().map_err(|_| unreachable!()),
        ))
        .unwrap()
}

pub const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Calculate Base64Encode(SHA512(password + salt) + salt)
pub fn hash_password(password: &str, salt: Option<&str>) -> String {
    let mut buf = String::new();
    let salt = salt.unwrap_or_else(|| {
        let mut rng = rand::rng();

        let random = (0..8)
            .map(|_| ALPHANUMERIC[rng.random_range(0..ALPHANUMERIC.len())] as char)
            .collect::<Vec<char>>();
        buf.extend(&random);
        buf.as_str()
    });

    let mut result = sha512(format!("{password}{salt}").as_bytes()).to_vec();
    result.extend_from_slice(salt.as_bytes());
    encode_block(&result)
}

pub fn check_password(password: &str, hash: &str) -> bool {
    let decoded = match decode_block(hash) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    if decoded.len() < 8 {
        return false;
    }

    let salt = match str::from_utf8(&decoded[decoded.len() - 8..]) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let expected_hash = hash_password(password, Some(salt));
    expected_hash == hash
}

#[macro_export]
macro_rules! required_header {
    ($request:expr, $header:expr) => {
        match $request.headers().get($header) {
            Some(header_value) => match header_value.to_str() {
                Ok(value) => value,
                Err(_) => return utils::bad_request(),
            },
            None => return utils::bad_request(),
        }
    };
}
