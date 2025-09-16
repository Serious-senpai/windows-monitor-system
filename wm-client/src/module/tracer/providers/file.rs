use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::{EventRecord, GUID, SchemaLocator};
use linked_hash_map::LinkedHashMap;
use log::warn;
use parking_lot::Mutex as BlockingMutex;
use windows::Win32::System::Diagnostics::Etw::{
    EVENT_TRACE_FLAG_DISK_FILE_IO, EVENT_TRACE_FLAG_FILE_IO_INIT,
};
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
    EVENT_TRACE_FLAG_DISK_FILE_IO.0 | EVENT_TRACE_FLAG_FILE_IO_INIT.0,
);
const _FILE_OBJECT_MAP_LIMIT: usize = 5000;

pub struct FileProviderWrapper {
    _file_object_map: BlockingMutex<LinkedHashMap<usize, String>>,
}

impl ProviderWrapper for FileProviderWrapper {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {
            _file_object_map: BlockingMutex::new(LinkedHashMap::with_capacity(
                _FILE_OBJECT_MAP_LIMIT,
            )),
        }
    }

    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.opcode() == 0
            || record.opcode() == 35
            || record.opcode() == 64
            || record.opcode() == 70
            || record.opcode() == 71
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);
                if record.opcode() <= 36 {
                    let file_object = parser
                        .try_parse::<Pointer>("FileObject")
                        .map_err(RuntimeError::from)?;
                    let file_name = parser
                        .try_parse::<String>("FileName")
                        .map_err(RuntimeError::from)?;

                    let mut map = self._file_object_map.lock();
                    map.remove(&file_object);
                    map.insert(*file_object, file_name);
                    if map.len() > _FILE_OBJECT_MAP_LIMIT {
                        let _ = map.pop_front();
                        map.shrink_to_fit();
                    }

                    return Ok(None);
                }

                let irp_ptr = parser
                    .try_parse::<Pointer>("IrpPtr")
                    .map_err(RuntimeError::from)?;
                let ttid = parser
                    .try_parse::<Pointer>("TTID")
                    .map_err(RuntimeError::from)?;
                let file_object = parser
                    .try_parse::<Pointer>("FileObject")
                    .map_err(RuntimeError::from)?;

                let file_name = if record.opcode() == 64 {
                    parser
                        .try_parse::<String>("OpenPath")
                        .map_err(RuntimeError::from)?
                } else {
                    let file_key = parser
                        .try_parse::<Pointer>("FileKey")
                        .map_err(RuntimeError::from)?;

                    let map = self._file_object_map.lock();
                    map.get(&file_key).cloned().unwrap_or_default()
                };

                let file_attributes = if record.opcode() == 64 {
                    parser
                        .try_parse::<u32>("FileAttributes")
                        .map_err(RuntimeError::from)?
                } else {
                    0
                };

                Ok(Some(Event::new(
                    record,
                    EventData::File {
                        irp_ptr: *irp_ptr,
                        ttid: *ttid,
                        file_object: *file_object,
                        file_name,
                        file_attributes,
                    },
                )))
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}
