use std::path::PathBuf;

use elasticsearch::http::Url;
use serde::{Deserialize, Serialize};
use wm_common::logger::LogLevel;

#[derive(Deserialize, Serialize)]
pub struct Elasticsearch {
    pub host: Url,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub port: u16,
    pub log_level: LogLevel,
    pub certificate: PathBuf,
    pub private_key: PathBuf,
    pub elasticsearch: Elasticsearch,
}
