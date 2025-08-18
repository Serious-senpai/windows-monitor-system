use std::collections::HashMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use url::Url;
use wm_common::logger::LogLevel;

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub server: Url,
    pub log_level: LogLevel,
    pub dns_resolver: HashMap<String, IpAddr>,
}
