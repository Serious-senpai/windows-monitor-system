use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::Utc;
use wm_common::schema::event::{CapturedEventRecord, Event, EventData};
use wm_common::schema::sysinfo::{CPUInfo, MemoryInfo, OSInfo, SystemInfo};

pub struct EventGenerator {
    _pool: Vec<Vec<u8>>,
    _index: AtomicUsize,
}

impl EventGenerator {
    pub fn new(pool_size: usize) -> Self {
        let mut pool = Vec::with_capacity(pool_size);
        for index in 0..pool_size {
            let system_info = Arc::new(SystemInfo::new(
                Arc::new(OSInfo {
                    full: format!("Windows 10 Pro Build {}", 19041 + (index % 100)),
                    kernel: format!("10.0.{}.0", 19041 + (index % 100)),
                    name: "Windows".to_string(),
                    platform: "x86_64-pc-windows-msvc".to_string(),
                    version: format!("10.0.{}", 19041 + (index % 100)),
                }),
                MemoryInfo {
                    memory_load: (index as u32 % 90) + 10,
                    total_physical: 16777216000 + (index as u64 % 8589934592),
                    available_physical: 8388608000 + (index as u64 % 4294967296),
                    total_page_file: 20971520000 + (index as u64 % 10737418240),
                    available_page_file: 10485760000 + (index as u64 % 5368709120),
                    total_virtual: 137438953472,
                    available_virtual: 137438953472 - (index as u64 % 1073741824),
                },
                CPUInfo {
                    usage: (index as f64 % 100.0).max(0.1),
                },
                format!("x86_64-{}", index % 10),
                format!("DESKTOP-{:06X}", index),
            ));

            let event_data = match index % 7 {
                0 => EventData::FileCreate {
                    file_object: 0x1000 + index,
                    options: index as u32,
                    attributes: 0x80 + (index as u32 % 256),
                    share_access: index as u32 % 8,
                    open_path: format!("C:\\temp\\file_{}.txt", index),
                },
                1 => EventData::FileInfo {
                    file_object: 0x2000 + index,
                    extra_info: 0x3000 + index,
                    info_class: (index as u32 % 50) + 1,
                    file_path: format!("C:\\data\\info_{}.dat", index),
                },
                2 => EventData::FileReadWrite {
                    offset: (index as u64) * 1024,
                    file_object: 0x4000 + index,
                    size: (index as u32 % 8192) + 1,
                    flags: index as u32 % 16,
                    file_path: format!("C:\\logs\\rw_{}.log", index),
                },
                3 => EventData::FileDelete {
                    file_path: format!("C:\\temp\\deleted_{}.tmp", index),
                },
                4 => EventData::Image {
                    image_base: 0x10000000 + (index * 0x1000),
                    image_size: 0x100000 + (index * 0x1000),
                    image_checksum: (index as u32).wrapping_mul(31),
                    file_name: format!("C:\\Program Files\\app_{}.dll", index),
                },
                5 => EventData::Process {
                    unique_process_key: 0x5000 + index,
                    process_id: (index as u32 % 30000) + 1000,
                    parent_id: (index as u32 % 1000) + 4,
                    session_id: index as u32 % 10,
                    exit_status: (index as i32) % 256,
                    directory_table_base: 0x6000 + index,
                    image_file_name: format!("process_{}.exe", index),
                    command_line: format!("process_{}.exe --arg{}", index, index),
                },
                _ => EventData::Registry {
                    initial_time: 132000000000000000 + (index as i64 * 10000000),
                    status: index,
                    index: index as u32,
                    key_handle: 0x7000 + index,
                    key_name: format!("HKEY_LOCAL_MACHINE\\SOFTWARE\\Test\\Key_{}", index),
                },
            };

            let event = Event {
                guid: format!("12345678-1234-1234-1234-{:012}", index),
                raw_timestamp: 132000000000000000 + (index as i64 * 10000000),
                process_id: (index as u32 % 30000) + 1000,
                thread_id: (index as u32 % 100) + 1,
                event_id: (index as u16 % 1000) + 1,
                opcode: (index as u8 % 100) + 1,
                data: event_data,
            };

            let captured_event = CapturedEventRecord {
                event,
                system: system_info.clone(),
                captured: Utc::now(),
            };

            pool.push(captured_event.serialize_to_vec());
        }

        Self {
            _pool: pool,
            _index: AtomicUsize::new(0),
        }
    }

    pub fn get_event(&self) -> &[u8] {
        let index = self._index.fetch_add(1, Ordering::Relaxed);
        &self._pool[index % self._pool.len()]
    }
}
