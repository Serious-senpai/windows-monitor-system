use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ferrisetw::EventRecord;
use serde::{Deserialize, Serialize};
use serde_json::json;
use windows::Wdk::Storage::FileSystem::{FileAllocationInformation, FileEndOfFileInformation};
use wm_generated::ecs::{
    ECS, ECS_Destination, ECS_Dll, ECS_Dll_CodeSignature, ECS_Event, ECS_File, ECS_Host,
    ECS_Host_Cpu, ECS_Host_Os, ECS_Process, ECS_Registry, ECS_Source,
};

use crate::schema::ecs_converter::file_attributes;
use crate::schema::sysinfo::SystemInfo;
use crate::utils::{split_command_line, windows_timestamp};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum EventData {
    FileCreate {
        file_object: usize,
        options: u32,
        attributes: u32,
        share_access: u32,
        open_path: String,
    },
    FileOperation {
        file_object: usize,
        extra_info: usize,
        info_class: u32,
        file_path: String,
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
            Self::FileCreate { .. } | Self::FileOperation { .. } => "file",
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

#[derive(Debug, Deserialize)]
pub struct CapturedEventRecord {
    pub event: Event,
    pub system: Arc<SystemInfo>,
    pub captured: DateTime<Utc>,
}

impl CapturedEventRecord {
    pub fn serialize_to_string(&self) -> String {
        unsafe {
            // JSON is always valid UTF-8, right?
            String::from_utf8_unchecked(self.serialize_to_vec())
        }
    }

    pub fn serialize_to_vec(&self) -> Vec<u8> {
        let mut vec = Vec::with_capacity(512); // According to serialization tests, 512 bytes is enough for most cases (usually they are 300-400 bytes)
        self.serialize_to_writer(&mut vec)
            .expect("Failed to serialize event record to buffer");
        vec
    }

    pub fn serialize_to_writer<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        writer.write_all(b"{\"event\":")?;
        serde_json::to_writer(&mut *writer, &self.event)?;
        writer.write_all(b",\"system\":")?;

        let buf = self.system.serialize_to_vec();
        writer.write_all(if buf.is_empty() { b"null" } else { buf })?;

        writer.write_all(b",\"captured\":")?;
        serde_json::to_writer(&mut *writer, &self.captured)?;
        writer.write_all(b"}")?;

        Ok(())
    }

    pub fn to_ecs(&self, ip: IpAddr) -> ECS {
        let mut os = ECS_Host_Os::new();
        os.family = Some(vec![self.system.os.platform.clone()]);
        os.full = Some(vec![self.system.os.full.clone()]);
        os.kernel = Some(vec![self.system.os.kernel.clone()]);
        os.name = Some(vec![self.system.os.name.clone()]);
        os.platform = Some(vec![self.system.os.platform.clone()]);
        os.type_ = Some(vec![self.system.os.platform.clone()]);
        os.version = Some(vec![self.system.os.version.clone()]);

        let mut cpu = ECS_Host_Cpu::new();
        cpu.usage = Some(self.system.cpu.usage);

        let mut host = ECS_Host::new();
        host.architecture = Some(vec![self.system.architecture.clone()]);
        host.cpu = Some(cpu);
        host.hostname = Some(vec![self.system.hostname.clone()]);
        host.id = Some(vec![ip.to_string()]);
        host.ip = Some(ip);
        host.name = Some(vec![self.system.hostname.clone()]);
        host.os = Some(os);

        let mut event = ECS_Event::new();
        event.code = Some(vec![
            self.event.event_id.to_string(),
            self.event.opcode.to_string(),
        ]);
        event.created = Some(self.captured);
        event.ingested = Some(Utc::now());
        event.kind = Some(vec!["event".to_string()]);
        event.module = Some(vec!["wm-client".to_string()]);
        event.original = Some(vec![self.serialize_to_string()]);
        event.provider = Some(vec!["kernel".to_string()]);

        let mut ecs = ECS::new(windows_timestamp(self.event.raw_timestamp));
        ecs.labels = Some(json!({"application": "windows-monitor"}));
        ecs.tags = Some(vec![self.event.data.event_type().into()]);
        ecs.host = Some(host);

        match &self.event.data {
            EventData::FileCreate {
                file_object,
                attributes,
                share_access,
                open_path,
                ..
            } => {
                event.action = Some(vec!["file-create".to_string()]);
                event.category = Some(vec!["file".to_string()]);
                event.type_ = Some(vec!["creation".to_string()]);

                let path = Path::new(open_path);

                let mut file = ECS_File::new();
                file.attributes = Some(file_attributes(*attributes));
                file.directory = path.parent().map(|s| vec![s.to_string_lossy().to_string()]);
                file.extension = path
                    .extension()
                    .map(|s| vec![s.to_string_lossy().to_string()]);
                file.inode = Some(vec![file_object.to_string()]);
                file.mode = Some(vec![format!("{share_access:o}")]);
                file.name = path
                    .file_name()
                    .map(|s| vec![s.to_string_lossy().to_string()]);
                file.path = Some(vec![open_path.clone()]);
                ecs.file = Some(file);
            }
            EventData::FileOperation {
                file_object,
                extra_info,
                info_class,
                file_path,
            } => {
                event.action = Some(vec![
                    match self.event.opcode {
                        69 => "file-set-info",
                        70 => "file-delete",
                        71 => "file-rename",
                        74 => "file-query-info",
                        75 => "file-system-control",
                        _ => "file-unknown",
                    }
                    .to_string(),
                ]);
                event.category = Some(vec!["file".to_string()]);
                event.type_ = Some(vec!["creation".to_string()]);

                let path = Path::new(file_path);

                let mut file = ECS_File::new();
                file.directory = path.parent().map(|s| vec![s.to_string_lossy().to_string()]);
                file.extension = path
                    .extension()
                    .map(|s| vec![s.to_string_lossy().to_string()]);
                file.inode = Some(vec![file_object.to_string()]);
                file.name = path
                    .file_name()
                    .map(|s| vec![s.to_string_lossy().to_string()]);
                file.path = Some(vec![file_path.clone()]);

                let info_class = *info_class as i32;
                file.size = if info_class == FileAllocationInformation.0
                    || info_class == FileEndOfFileInformation.0
                {
                    i64::try_from(*extra_info).ok()
                } else {
                    None
                };
                ecs.file = Some(file);
            }
            EventData::Image { file_name, .. } => {
                event.action = Some(vec![
                    match self.event.opcode {
                        2 => "image-unload",
                        10 => "image-load",
                        _ => "image-unknown",
                    }
                    .to_string(),
                ]);
                event.category = Some(vec!["library".to_string()]);
                event.type_ = Some(vec![
                    match self.event.opcode {
                        2 => "end",
                        10 => "start",
                        _ => "info",
                    }
                    .to_string(),
                ]);

                let path = Path::new(file_name);

                let mut signature = ECS_Dll_CodeSignature::new();
                signature.exists = Some(false);

                let mut dll = ECS_Dll::new();
                dll.code_signature = Some(signature);
                dll.name = path
                    .file_name()
                    .map(|s| vec![s.to_string_lossy().to_string()]);
                dll.path = Some(vec![file_name.clone()]);

                ecs.dll = Some(dll);
            }
            EventData::Process {
                process_id,
                exit_status,
                image_file_name,
                command_line,
                ..
            } => {
                event.action = Some(vec![
                    match self.event.opcode {
                        1 => "process-start",
                        2 => "process-end",
                        _ => "process-unknown",
                    }
                    .to_string(),
                ]);
                event.category = Some(vec!["process".to_string()]);
                event.type_ = Some(vec![
                    match self.event.opcode {
                        1 => "start",
                        2 => "end",
                        _ => "info",
                    }
                    .to_string(),
                ]);

                let args = split_command_line(command_line);
                let args_count = args.len();

                let mut process = ECS_Process::new();
                process.args = Some(args);
                process.args_count = args_count.try_into().ok();
                process.command_line = Some(vec![command_line.clone()]);
                process.executable = Some(vec![image_file_name.clone()]);
                process.exit_code = Some(i64::from(*exit_status));
                process.pid = Some(i64::from(*process_id));
                ecs.process = Some(process);
            }
            EventData::Registry { key_name, .. } => {
                event.action = Some(vec![
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
                ]);
                event.category = Some(vec!["registry".to_string()]);
                event.type_ = Some(vec![
                    match self.event.opcode {
                        10 | 22 => "creation",
                        12 | 15 | 23 => "deletion",
                        14 | 20 | 21 => "change",
                        _ => "info",
                    }
                    .to_string(),
                ]);

                // let path = Path::new(key_name);

                let mut registry = ECS_Registry::new();
                registry.key = Some(vec![key_name.clone()]);
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
                event.action = Some(vec![
                    match self.event.opcode {
                        10 => "udp-send",
                        11 => "udp-receive",
                        12 => "tcp-connect",
                        13 => "tcp-disconnect",
                        15 => "tcp-accept",
                        _ => "tcp-udp-unknown",
                    }
                    .to_string(),
                ]);
                event.category = Some(vec!["network".to_string()]);
                event.type_ = Some(vec!["connection".to_string()]);

                let mut source = ECS_Source::new();
                source.address = Some(vec![saddr.to_string()]);
                source.bytes = Some(i64::from(*size));
                source.ip = Some(*saddr);
                source.port = Some(i64::from(*sport));
                ecs.source = Some(source);

                let mut destination = ECS_Destination::new();
                destination.address = Some(vec![daddr.to_string()]);
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
