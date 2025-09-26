use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;
use wm_common::logger::LogLevel;

#[derive(Deserialize, Serialize)]
pub struct RabbitMQ {
    pub host: Url,
}

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub port: u16,
    pub log_level: LogLevel,
    pub certificate: PathBuf,
    pub private_key: PathBuf,
    pub rabbitmq: RabbitMQ,
}
