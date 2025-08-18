use std::net::IpAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::sysinfo::SystemInfo;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum EventData {
    File {
        file_object: usize,
        file_name: String,
    },
    Image {
        image_base: usize,
        image_size: usize,
        process_id: u32,
        image_checksum: u32,
        file_name: String,
    },
    Process {
        unique_process_key: usize,
        process_id: u32,
        parent_id: u32,
        session_id: u32,
        exit_status: i32,
        directory_table_base: usize,
        image_file_name: String,
        command_line: String,
    },
    TcpIp {
        pid: u32,
        size: u32,
        daddr: IpAddr,
        saddr: IpAddr,
        dport: u16,
        sport: u16,
    },
    UdpIp {
        pid: u32,
        size: u32,
        daddr: IpAddr,
        saddr: IpAddr,
        dport: u16,
        sport: u16,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    pub guid: String,
    pub process_id: u32,
    pub thread_id: u32,
    pub event_id: u16,
    pub opcode: u8,
    pub data: EventData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CapturedEventRecord {
    pub event: Event,
    pub system: Arc<SystemInfo>,
    pub buffer_length: usize,
}
