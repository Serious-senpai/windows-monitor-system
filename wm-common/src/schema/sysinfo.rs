use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct OSInfo {
    pub full: String,
    pub kernel: String,
    pub name: String,
    pub platform: String,
    pub version: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryInfo {
    pub memory_load: u32,
    pub total_physical: u64,
    pub available_physical: u64,
    pub total_page_file: u64,
    pub available_page_file: u64,
    pub total_virtual: u64,
    pub available_virtual: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CPUInfo {
    pub usage: f64,
}

impl CPUInfo {
    pub fn from_ckpt(before: &(u64, u64, u64), after: &(u64, u64, u64)) -> Self {
        let idle = after.0 - before.0;
        let kernel = after.1 - before.1;
        let user = after.2 - before.2;
        let total = kernel + user;

        let usage = if total == 0 {
            0.0
        } else {
            (total - idle) as f64 * 100.0 / total as f64
        };

        Self { usage }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SystemInfo {
    #[serde(skip)]
    _pre_serialize: Vec<u8>,

    pub os: Arc<OSInfo>,
    pub memory: MemoryInfo,
    pub cpu: CPUInfo,
    pub architecture: String,
    pub hostname: String,
}

impl SystemInfo {
    pub fn new(
        os: Arc<OSInfo>,
        memory: MemoryInfo,
        cpu: CPUInfo,
        architecture: String,
        hostname: String,
    ) -> Self {
        let mut this = Self {
            _pre_serialize: vec![],
            os,
            memory,
            cpu,
            architecture,
            hostname,
        };

        this._pre_serialize = serde_json::to_vec(&this).unwrap_or_default();
        this
    }

    pub fn serialize_to_vec(&self) -> &[u8] {
        &self._pre_serialize
    }
}
