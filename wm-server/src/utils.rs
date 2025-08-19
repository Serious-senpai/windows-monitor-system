use std::str;

use openssl::base64::{decode_block, encode_block};
use openssl::sha::sha512;
use rand::Rng;

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
