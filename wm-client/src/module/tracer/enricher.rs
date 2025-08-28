use std::env::consts::OS;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::{MINIMUM_CPU_UPDATE_INTERVAL, System};
use tokio::time::sleep;
use wm_common::schema::sysinfo::{CPUInfo, MemoryInfo, OSInfo, SystemInfo};

pub struct BlockingSystemInfo {
    _system_refresh: Duration,
    _last_update: Instant,
    _system: System,
    _info: Arc<SystemInfo>,
}

impl BlockingSystemInfo {
    pub async fn async_new(refresh: Duration) -> Self {
        let mut system = System::new_all();
        sleep(MINIMUM_CPU_UPDATE_INTERVAL).await;

        let sysinfo = Self::_fetch_sysinfo(&mut system);
        Self {
            _system_refresh: refresh,
            _last_update: Instant::now(),
            _system: system,
            _info: sysinfo,
        }
    }

    fn _fetch_sysinfo(system: &mut System) -> Arc<SystemInfo> {
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

        Arc::new(SystemInfo { os, memory, cpus })
    }

    pub fn system_info(&mut self) -> Arc<SystemInfo> {
        if self._last_update.elapsed() > self._system_refresh {
            self._info = Self::_fetch_sysinfo(&mut self._system);
            self._last_update = Instant::now();
        }

        self._info.clone()
    }
}

pub struct BlockingEventCounter {
    _total: u128,
    _eps: f64,
    _last_record: Instant,
}

impl BlockingEventCounter {
    fn new() -> Self {
        Self {
            _total: 0,
            _eps: 0.0,
            _last_record: Instant::now(),
        }
    }

    pub fn record(&mut self) -> u128 {
        let elapsed = self._last_record.elapsed().as_secs_f64();
        self._eps = if elapsed < 1.0 {
            if self._eps == 0.0 {
                1.0 / elapsed
            } else {
                1.0 + self._eps * (1.0 - elapsed)
            }
        } else {
            1.0 / elapsed
        };

        self._total = self._total.saturating_add(1);
        self._last_record = Instant::now();

        self._total
    }

    pub fn eps(&self) -> f64 {
        self._eps
    }
}

pub struct BlockingEventEnricher {
    pub system: BlockingSystemInfo,
    pub counter: BlockingEventCounter,
}

impl BlockingEventEnricher {
    pub async fn async_new(system_refresh: Duration) -> Self {
        Self {
            system: BlockingSystemInfo::async_new(system_refresh).await,
            counter: BlockingEventCounter::new(),
        }
    }
}
