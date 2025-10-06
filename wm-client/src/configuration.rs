use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use ferrisetw::trace::{LoggingMode, TraceProperties};
use serde::{Deserialize, Serialize};
use url::Url;
use wm_common::logger::LogLevel;

fn _service_name() -> String {
    "Windows Monitor Agent Service".to_string()
}

fn _trace_name() -> TraceName {
    TraceName {
        kernel: "Windows Monitor Kernel Tracer".to_string(),
        user: "Windows Monitor User Tracer".to_string(),
    }
}

fn _password_registry_key() -> String {
    r"SOFTWARE\WindowsMonitor\CertificatePassword".to_string()
}

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
pub struct TraceSettings {
    pub buffer_size: u32,
    pub min_buffer: u32,
    pub max_buffer: u32,
}

impl TraceSettings {
    pub fn to_trace_properties(&self) -> TraceProperties {
        // https://learn.microsoft.com/en-us/windows/win32/api/evntrace/ns-evntrace-event_trace_properties
        TraceProperties {
            buffer_size: self.buffer_size,
            min_buffer: self.min_buffer,
            max_buffer: self.max_buffer,
            flush_timer: Duration::from_secs(1),
            log_file_mode: LoggingMode::EVENT_TRACE_REAL_TIME_MODE,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    #[serde(skip, default = "_service_name")]
    pub service_name: String,
    #[serde(skip, default = "_trace_name")]
    pub trace_name: TraceName,
    #[serde(skip, default = "_password_registry_key")]
    pub password_registry_key: String,

    pub trace: TraceSettings,
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
