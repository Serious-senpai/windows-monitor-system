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

pub struct FileProviderWrapper;

impl ProviderWrapper for FileProviderWrapper {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
    }

    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.opcode() == 0 || record.opcode() == 32 || record.opcode() == 35
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

                Ok(Event {
                    guid: format!("{:?}", record.provider_id()),
                    raw_timestamp: record.raw_timestamp(),
                    process_id: record.process_id(),
                    thread_id: record.thread_id(),
                    event_id: record.event_id(),
                    opcode: record.opcode(),
                    data: EventData::File {
                        file_object: *file_object,
                        file_name,
                    },
                })
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}
