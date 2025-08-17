use serde::{Deserialize, Serialize};
use url::Url;
use wm_common::logger::LogLevel;

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub server: Url,
    pub log_level: LogLevel,
}
