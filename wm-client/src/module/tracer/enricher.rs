use std::env::consts::OS;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::warn;
use sysinfo::{MINIMUM_CPU_UPDATE_INTERVAL, System};
use tokio::time::sleep;
use wm_common::schema::sysinfo::{CPUInfo, OSInfo, SystemInfo};
use wm_common::sysinfo::{get_system_times, memory_status};
use wm_common::utils::get_computer_name;

pub struct BlockingSystemInfo {
    _system_refresh: Duration,
    _last_update: Instant,
    _info: Arc<SystemInfo>,
    _os_info: Arc<OSInfo>,
    _last_cpu_ckpt: (u64, u64, u64),
}

impl BlockingSystemInfo {
    pub async fn async_new(refresh: Duration) -> Self {
        if refresh < MINIMUM_CPU_UPDATE_INTERVAL {
            warn!(
                "System info refresh interval is too low (should be at least {}s)",
                MINIMUM_CPU_UPDATE_INTERVAL.as_secs_f64()
            );
        }

        let cpu_ckpt = get_system_times().unwrap_or_default();
        let os_info = Arc::new(OSInfo {
            full: System::long_os_version().unwrap_or_default(),
            kernel: System::kernel_version().unwrap_or_default(),
            name: System::name().unwrap_or_default(),
            platform: OS.to_string(),
            version: System::os_version().unwrap_or_default(),
        });

        sleep(MINIMUM_CPU_UPDATE_INTERVAL).await;
        let (cpu_ckpt, sysinfo) = Self::_fetch_sysinfo(&cpu_ckpt, &os_info)
            .expect("Failed to calculate initial system info");

        Self {
            _system_refresh: refresh,
            _last_update: Instant::now(),
            _info: sysinfo,
            _os_info: os_info,
            _last_cpu_ckpt: cpu_ckpt,
        }
    }

    fn _fetch_sysinfo(
        last_cpu_ckpt: &(u64, u64, u64),
        os_info: &Arc<OSInfo>,
    ) -> Option<((u64, u64, u64), Arc<SystemInfo>)> {
        let cpu_ckpt = match get_system_times() {
            Ok(ckpt) => ckpt,
            Err(e) => {
                warn!("Failed to get CPU times: {e}");
                return None;
            }
        };
        let cpu = CPUInfo::from_ckpt(last_cpu_ckpt, &cpu_ckpt);
        let memory = match memory_status() {
            Ok(mem) => mem,
            Err(e) => {
                warn!("Failed to get memory status: {e}");
                return None;
            }
        };

        Some((
            cpu_ckpt,
            Arc::new(SystemInfo::new(
                os_info.clone(),
                memory,
                cpu,
                if cfg!(target_arch = "x86_64") {
                    "x86_64"
                } else if cfg!(target_arch = "x86") {
                    "x86"
                } else {
                    "unknown"
                }
                .to_string(),
                get_computer_name().unwrap_or_else(|_| "unknown".to_string()),
            )),
        ))
    }

    pub fn system_info(&mut self) -> Arc<SystemInfo> {
        if self._last_update.elapsed() > self._system_refresh
            && let Some(packed) = Self::_fetch_sysinfo(&self._last_cpu_ckpt, &self._os_info)
        {
            (self._last_cpu_ckpt, self._info) = packed;
            self._last_update = Instant::now();
        }

        self._info.clone()
    }
}

pub struct BlockingEventEnricher {
    pub system: BlockingSystemInfo,
}

impl BlockingEventEnricher {
    pub async fn async_new(system_refresh: Duration) -> Self {
        Self {
            system: BlockingSystemInfo::async_new(system_refresh).await,
        }
    }
}
