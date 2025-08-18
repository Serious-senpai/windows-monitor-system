use std::env::consts::OS;
use std::sync::Arc;
use std::time::Duration;

use log::trace;
use serde::{Deserialize, Serialize};
use sysinfo::System;
use tokio::sync::{Mutex, OnceCell};
use tokio::time::sleep;

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

static _SYSTEM: OnceCell<Mutex<System>> = OnceCell::const_new();
static _SYSTEM_INFO: Mutex<Option<Arc<SystemInfo>>> = Mutex::const_new(None);

impl SystemInfo {
    pub async fn fetch() -> Arc<Self> {
        let system_info = {
            let mut info = _SYSTEM_INFO.lock().await;
            if let Some(info) = info.as_ref() {
                // this `return` is for `fetch()`, see https://doc.rust-lang.org/std/keyword.async.html#control-flow
                return info.clone();
            }

            trace!("Updating system info");
            let mut system = _SYSTEM
                .get_or_init(async || Mutex::new(System::new_all()))
                .await
                .lock()
                .await;
            system.refresh_all();

            let os = OSInfo {
                full: System::long_os_version().unwrap_or_default(),
                kernel: System::kernel_version().unwrap_or_default(),
                name: System::name().unwrap_or_default(),
                platform: OS.to_string(),
                version: System::os_version().unwrap_or_default(),
            };

            let memory = MemoryInfo {
                total_mem: system.total_memory(),
                used_mem: system.used_memory(),
                total_swap: system.total_swap(),
                used_swap: system.used_swap(),
            };

            let cpus = system
                .cpus()
                .iter()
                .map(|cpu| CPUInfo {
                    brand: cpu.brand().to_string(),
                    usage: cpu.cpu_usage(),
                    frequency: cpu.frequency(),
                    name: cpu.name().to_string(),
                    vendor_id: cpu.vendor_id().to_string(),
                })
                .collect();

            let created = Arc::new(Self { os, memory, cpus });
            *info = Some(created.clone());
            created
        };

        // Refresh data every 1 second
        tokio::spawn(async {
            sleep(Duration::from_secs(1)).await;

            let mut info = _SYSTEM_INFO.lock().await;
            *info = None;
        });

        system_info
    }
}
