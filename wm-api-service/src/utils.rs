use std::collections::HashMap;
use std::net::IpAddr;

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

pub fn append_client_ip(buffer: &mut Vec<u8>, ip: IpAddr) {
    let ip_native_order = match ip {
        IpAddr::V4(ipv4) => u128::from(ipv4.to_bits()),
        IpAddr::V6(ipv6) => ipv6.to_bits(),
    };
    buffer.extend_from_slice(&ip_native_order.to_be_bytes());
    buffer.push(u8::from(matches!(ip, IpAddr::V4(_))));
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
