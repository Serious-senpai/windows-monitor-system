use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ferrisetw::EventRecord;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wm_generated::ecs::{
    ECS, ECS_Destination, ECS_Dll, ECS_Event, ECS_File, ECS_Host, ECS_Host_Cpu, ECS_Host_Os,
    ECS_Process, ECS_Registry, ECS_Source,
};

use crate::schema::sysinfo::SystemInfo;
use crate::utils::windows_timestamp;

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
    Registry {
        initial_time: i64,
        status: usize,
        index: u32,
        key_handle: usize,
        key_name: String,
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

impl EventData {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Image { .. } => "image",
            Self::Process { .. } => "process",
            Self::Registry { .. } => "registry",
            Self::TcpIp { .. } => "tcpip",
            Self::UdpIp { .. } => "udpip",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    pub guid: String,
    pub raw_timestamp: i64,
    pub process_id: u32,
    pub thread_id: u32,
    pub event_id: u16,
    pub opcode: u8,
    pub data: EventData,
}

impl Event {
    pub fn new(record: &EventRecord, data: EventData) -> Self {
        Self {
            guid: format!("{:?}", record.provider_id()),
            raw_timestamp: record.raw_timestamp(),
            process_id: record.process_id(),
            thread_id: record.thread_id(),
            event_id: record.event_id(),
            opcode: record.opcode(),
            data,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CapturedEventRecord {
    pub event: Event,
    pub system: Arc<SystemInfo>,
    pub captured: DateTime<Utc>,
}

impl CapturedEventRecord {
    pub fn to_ecs(&self, ip: IpAddr) -> ECS {
        let mut os = ECS_Host_Os::new();
        os.family = Some(self.system.os.platform.clone());
        os.full = Some(self.system.os.full.clone());
        os.kernel = Some(self.system.os.kernel.clone());
        os.name = Some(self.system.os.name.clone());
        os.platform = Some(self.system.os.platform.clone());
        os.type_ = Some(self.system.os.platform.clone());
        os.version = Some(self.system.os.version.clone());

        let mut cpu = ECS_Host_Cpu::new();
        cpu.usage = Some(self.system.cpu.usage);

        let mut host = ECS_Host::new();
        host.architecture = Some(self.system.architecture.clone());
        host.hostname = Some(self.system.hostname.clone());
        host.id = Some(ip.to_string());
        host.ip = Some(ip);
        host.name = Some(self.system.hostname.clone());
        host.os = Some(os);

        let mut event = ECS_Event::new();
        event.created = Some(self.captured);
        event.ingested = Some(Utc::now());
        event.kind = Some("event".to_string());
        event.module = Some("wm-client".to_string());
        event.original = Some(serde_json::to_string(self).unwrap());
        event.provider = Some("kernel".to_string());

        let mut ecs = ECS::new(windows_timestamp(self.event.raw_timestamp));
        ecs.labels = Some(json!({"application": "windows-monitor"}));
        ecs.tags = Some(self.event.data.event_type().into());
        ecs.host = Some(host);

        match &self.event.data {
            EventData::File {
                file_object,
                file_name,
            } => {
                event.action = Some(
                    match self.event.opcode {
                        0 => "file-name",
                        32 => "file-create",
                        35 => "file-delete",
                        _ => "file-unknown",
                    }
                    .to_string(),
                );
                event.category = Some("file".to_string());
                event.outcome = Some("success".to_string());
                event.type_ = Some(
                    match self.event.opcode {
                        32 => "creation",
                        35 => "deletion",
                        _ => "info",
                    }
                    .to_string(),
                );

                let path = Path::new(file_name);

                let mut file = ECS_File::new();
                file.inode = Some(file_object.to_string());
                file.name = path.file_name().map(|s| s.to_string_lossy().to_string());
                file.path = Some(file_name.clone());
                ecs.file = Some(file);
            }
            EventData::Image { file_name, .. } => {
                event.action = Some(
                    match self.event.opcode {
                        2 => "image-unload",
                        10 => "image-load",
                        _ => "image-unknown",
                    }
                    .to_string(),
                );
                event.category = Some("library".to_string());
                event.outcome = Some("success".to_string());
                event.type_ = Some(
                    match self.event.opcode {
                        2 => "end",
                        10 => "start",
                        _ => "info",
                    }
                    .to_string(),
                );

                let path = Path::new(file_name);

                let mut dll = ECS_Dll::new();
                dll.name = path.file_name().map(|s| s.to_string_lossy().to_string());
                dll.path = Some(file_name.clone());
                ecs.dll = Some(dll);
            }
            EventData::Process {
                process_id,
                exit_status,
                image_file_name,
                command_line,
                ..
            } => {
                event.action = Some(
                    match self.event.opcode {
                        1 => "process-start",
                        2 => "process-end",
                        _ => "process-unknown",
                    }
                    .to_string(),
                );
                event.category = Some("process".to_string());
                event.outcome = Some("success".to_string());
                event.type_ = Some(
                    match self.event.opcode {
                        1 => "start",
                        2 => "end",
                        _ => "info",
                    }
                    .to_string(),
                );

                let mut process = ECS_Process::new();
                process.command_line = Some(command_line.clone());
                process.executable = Some(image_file_name.clone());
                process.exit_code = Some(i64::from(*exit_status));
                process.pid = Some(i64::from(*process_id));
                ecs.process = Some(process);
            }
            EventData::Registry { key_name, .. } => {
                event.action = Some(
                    match self.event.opcode {
                        10 | 22 => "registry-create-key",
                        12 | 23 => "registry-delete-key",
                        14 => "registry-set-value",
                        15 => "registry-delete-value",
                        20 => "registry-set-info",
                        21 => "registry-flush-key",
                        _ => "registry-unknown",
                    }
                    .to_string(),
                );
                event.category = Some("registry".to_string());
                event.outcome = Some("success".to_string());
                event.type_ = Some(
                    match self.event.opcode {
                        10 | 22 => "creation",
                        12 | 15 | 23 => "deletion",
                        14 | 20 | 21 => "change",
                        _ => "info",
                    }
                    .to_string(),
                );

                // let path = Path::new(key_name);

                let mut registry = ECS_Registry::new();
                registry.key = Some(key_name.clone());
                ecs.registry = Some(registry);
            }
            EventData::TcpIp {
                size,
                daddr,
                saddr,
                dport,
                sport,
                ..
            }
            | EventData::UdpIp {
                size,
                daddr,
                saddr,
                dport,
                sport,
                ..
            } => {
                event.action = Some(
                    match self.event.opcode {
                        10 => "udp-send",
                        11 => "udp-receive",
                        12 => "tcp-connect",
                        13 => "tcp-disconnect",
                        15 => "tcp-accept",
                        _ => "tcp-udp-unknown",
                    }
                    .to_string(),
                );
                event.category = Some("network".to_string());
                event.outcome = Some("success".to_string());
                event.type_ = Some("connection".to_string());

                let mut source = ECS_Source::new();
                source.address = Some(saddr.to_string());
                source.bytes = Some(i64::from(*size));
                source.ip = Some(*saddr);
                source.port = Some(i64::from(*sport));
                ecs.source = Some(source);

                let mut destination = ECS_Destination::new();
                destination.address = Some(daddr.to_string());
                destination.bytes = Some(i64::from(*size));
                destination.ip = Some(*daddr);
                destination.port = Some(i64::from(*dport));
                ecs.destination = Some(destination);
            }
        }

        ecs.event = Some(event);

        ecs
    }
}
