use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;
use wm_common::logger::LogLevel;

#[derive(Deserialize, Serialize)]
pub struct EventPostSettings {
    pub concurrency_limit: usize,
    pub flush_limit: usize,
}

#[derive(Deserialize, Serialize)]
pub struct TraceName {
    pub kernel: String,
    pub user: String,
}

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub service_name: String,
    pub trace_name: TraceName,
    pub password_registry_key: String,
    pub server: Url,
    pub zstd_compression_level: i32,
    pub system_refresh_interval_seconds: f64,
    pub backup_directory: PathBuf,
    pub log_level: LogLevel,
    pub message_queue_limit: usize,
    pub dns_resolver: HashMap<String, IpAddr>,
    pub event_post: EventPostSettings,
    pub runtime_threads: usize,
}
