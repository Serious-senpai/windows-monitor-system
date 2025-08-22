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
    pub total_mem: u64,
    pub used_mem: u64,
    pub total_swap: u64,
    pub used_swap: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CPUInfo {
    pub brand: String,
    pub usage: f32,
    pub frequency: u64,
    pub name: String,
    pub vendor_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SystemInfo {
    pub os: OSInfo,
    pub memory: MemoryInfo,
    pub cpus: Vec<CPUInfo>,
}
