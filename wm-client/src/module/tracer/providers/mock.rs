use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::{EventRecord, GUID, SchemaLocator};
use windows::Win32::System::Diagnostics::Etw::EVENT_TRACE_FLAG_DISK_FILE_IO;
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::ProviderWrapper;

const _PROVIDER: KernelProvider = KernelProvider::new(
    GUID::from_values(
        0x90cbdc39,
        0x4a3e,
        0x11d1,
        [0x84, 0xf4, 0x00, 0x00, 0xf8, 0x04, 0x64, 0xe3],
    ),
    EVENT_TRACE_FLAG_DISK_FILE_IO.0,
);

pub struct MockProviderWrapper {
    _pid: u32,
}

impl MockProviderWrapper {
    pub fn with_pid(pid: u32) -> Self {
        Self { _pid: pid }
    }
}

impl ProviderWrapper for MockProviderWrapper {
    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.process_id() == self._pid
            && (record.opcode() == 0 || record.opcode() == 32 || record.opcode() == 35)
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Event, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);
                let file_object = parser
                    .try_parse::<Pointer>("FileObject")
                    .map_err(RuntimeError::from)?;
                let file_name = parser
                    .try_parse::<String>("FileName")
                    .map_err(RuntimeError::from)?;

                Ok(Event::new(
                    record,
                    EventData::File {
                        file_object: *file_object,
                        file_name,
                    },
                ))
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}
