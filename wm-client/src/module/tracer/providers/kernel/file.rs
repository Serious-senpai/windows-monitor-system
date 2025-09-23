use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::{EventRecord, GUID, SchemaLocator};
use lru::LruCache;
use parking_lot::Mutex as BlockingMutex;
use windows::Win32::System::Diagnostics::Etw::{
    EVENT_TRACE_FLAG_DISK_FILE_IO, EVENT_TRACE_FLAG_FILE_IO_INIT,
};
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::{KernelProviderWrapper, ProviderWrapper};

pub struct FileProviderWrapper {
    _mapping: BlockingMutex<LruCache<usize, String>>,
}

impl FileProviderWrapper {
    const _PROVIDER: KernelProvider = KernelProvider::new(
        GUID::from_values(
            0x90cbdc39,
            0x4a3e,
            0x11d1,
            [0x84, 0xf4, 0x00, 0x00, 0xf8, 0x04, 0x64, 0xe3],
        ),
        EVENT_TRACE_FLAG_DISK_FILE_IO.0 | EVENT_TRACE_FLAG_FILE_IO_INIT.0,
    );

    pub fn new(cache_size: usize) -> Self {
        Self {
            _mapping: BlockingMutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or_else(|| panic!("{} > 0", cache_size)),
            )),
        }
    }
}

impl ProviderWrapper for FileProviderWrapper {
    fn filter(&self, record: &EventRecord) -> bool {
        record.opcode() == 0
            || record.opcode() == 32
            || record.opcode() == 35
            || record.opcode() == 64
            || record.opcode() == 69
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
                match record.opcode() {
                    0 | 32 | 35 => {
                        let file_object = parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?;
                        let file_name = parser
                            .try_parse::<String>("FileName")
                            .map_err(RuntimeError::from)?;

                        match self._mapping.try_lock() {
                            Some(mut mapping) => {
                                mapping.put(*file_object, file_name.clone());
                            }
                            None => Err(RuntimeError::new(
                                "File I/O mapping mutex should never block",
                            ))?,
                        }

                        Ok(None)
                    }
                    64 => {
                        let file_object = parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?;
                        let options = parser
                            .try_parse::<u32>("CreateOptions")
                            .map_err(RuntimeError::from)?;
                        let attributes = parser
                            .try_parse::<u32>("FileAttributes")
                            .map_err(RuntimeError::from)?;
                        let share_access = parser
                            .try_parse::<u32>("ShareAccess")
                            .map_err(RuntimeError::from)?;
                        let open_path = parser
                            .try_parse::<String>("OpenPath")
                            .map_err(RuntimeError::from)?;

                        Ok(Some(Event::new(
                            record,
                            EventData::FileCreate {
                                file_object: *file_object,
                                options,
                                attributes,
                                share_access,
                                open_path,
                            },
                        )))
                    }
                    69 | 70 | 71 => {
                        let file_object = parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?;
                        let file_key = parser
                            .try_parse::<Pointer>("FileKey")
                            .map_err(RuntimeError::from)?;
                        let extra_info = parser
                            .try_parse::<Pointer>("ExtraInfo")
                            .map_err(RuntimeError::from)?;
                        let info_class = parser
                            .try_parse::<u32>("InfoClass")
                            .map_err(RuntimeError::from)?;

                        match self._mapping.try_lock() {
                            Some(mut mapping) => match mapping.get(&file_key).cloned() {
                                Some(file_path) => Ok(Some(Event::new(
                                    record,
                                    EventData::FileOperation {
                                        file_object: *file_object,
                                        extra_info: *extra_info,
                                        info_class,
                                        file_path,
                                    },
                                ))),
                                None => Ok(None),
                            },
                            None => Err(RuntimeError::new(
                                "File I/O mapping mutex should never block",
                            ))?,
                        }
                    }
                    other => Err(RuntimeError::new(format!("Unexpected opcode {other}")))?,
                }
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}

impl KernelProviderWrapper for FileProviderWrapper {
    fn provider(&self) -> &KernelProvider {
        &Self::_PROVIDER
    }
}
