use std::collections::HashMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use url::Url;
use wm_common::logger::LogLevel;

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub service_name: String,
    pub windows_credential_manager_key: String,
    pub server: Url,
    pub events_per_request: usize,
    pub zstd_compression_level: i32,
    pub log_level: LogLevel,
    pub dns_resolver: HashMap<String, IpAddr>,
}
