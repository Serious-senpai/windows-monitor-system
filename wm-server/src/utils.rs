use std::collections::HashMap;

use hyper::Request;
use url::form_urlencoded;

pub fn parse_query<T>(request: &Request<T>) -> Vec<(String, String)> {
    let query = request.uri().query().unwrap_or_default();
    form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

pub fn parse_query_map<T>(request: &Request<T>) -> HashMap<String, String> {
    let query = request.uri().query().unwrap_or_default();
    form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

#[macro_export]
macro_rules! required_header {
    ($request:expr, $header:expr) => {
        match $request.headers().get($header) {
            Some(header_value) => match header_value.to_str() {
                Ok(value) => value,
                Err(_) => {
                    return ResponseBuilder::message(StatusCode::BAD_REQUEST, "Invalid header value")
                }
            },
            None => {
                return ResponseBuilder::message(StatusCode::BAD_REQUEST, "Missing required header")
            }
        }
    };
}
