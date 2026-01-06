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

use crate::debug_parser;
use crate::module::tracer::providers::fast_parser::FastParser;
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
                NonZeroUsize::new(cache_size).unwrap_or_else(|| panic!("{cache_size} > 0")),
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
            || record.opcode() == 74
            || record.opcode() == 75
            || record.opcode() == 67
            || record.opcode() == 68
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>> {
        let mut fast = FastParser::new(record);
        match record.opcode() {
            0 | 32 | 35 => {
                let file_object = fast.try_read::<usize>()?;
                let file_name = fast.try_read_utf16()?;

                if cfg!(debug_assertions) {
                    debug_parser!(parser, record, schema_locator);
                    assert_eq!(
                        file_object,
                        *parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        file_name,
                        parser
                            .try_parse::<String>("FileName")
                            .map_err(RuntimeError::from)?
                    );
                }

                match self._mapping.try_lock() {
                    Some(mut mapping) => {
                        mapping.put(file_object, file_name.clone());
                    }
                    None => Err(RuntimeError::new(
                        "File I/O mapping mutex should never block",
                    ))?,
                }

                if record.opcode() == 35 {
                    Ok(Some(Event::new(
                        record,
                        EventData::FileDelete {
                            file_path: file_name,
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            64 => {
                fast.skip(8);
                let file_object = fast.try_read::<usize>()?;
                fast.skip(4);
                let options = fast.try_read::<u32>()?;
                let attributes = fast.try_read::<u32>()?;
                let share_access = fast.try_read::<u32>()?;
                let open_path = fast.try_read_utf16()?;

                if cfg!(debug_assertions) {
                    debug_parser!(parser, record, schema_locator);
                    assert_eq!(
                        file_object,
                        *parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        options,
                        parser
                            .try_parse::<u32>("CreateOptions")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        attributes,
                        parser
                            .try_parse::<u32>("FileAttributes")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        share_access,
                        parser
                            .try_parse::<u32>("ShareAccess")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        open_path,
                        parser
                            .try_parse::<String>("OpenPath")
                            .map_err(RuntimeError::from)?
                    );
                }

                Ok(Some(Event::new(
                    record,
                    EventData::FileCreate {
                        file_object,
                        options,
                        attributes,
                        share_access,
                        open_path,
                    },
                )))
            }
            69 | 70 | 71 | 74 | 75 => {
                fast.skip(8);
                let file_object = fast.try_read::<usize>()?;
                let file_key = fast.try_read::<usize>()?;
                let extra_info = fast.try_read::<usize>()?;
                fast.skip(4);
                let info_class = fast.try_read::<u32>()?;

                if cfg!(debug_assertions) {
                    debug_parser!(parser, record, schema_locator);
                    assert_eq!(
                        file_object,
                        *parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        file_key,
                        *parser
                            .try_parse::<Pointer>("FileKey")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        extra_info,
                        *parser
                            .try_parse::<Pointer>("ExtraInfo")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        info_class,
                        parser
                            .try_parse::<u32>("InfoClass")
                            .map_err(RuntimeError::from)?
                    );
                }

                match self._mapping.try_lock() {
                    Some(mut mapping) => match mapping.get(&file_key).cloned() {
                        Some(file_path) => Ok(Some(Event::new(
                            record,
                            EventData::FileInfo {
                                file_object,
                                extra_info,
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
            67 | 68 => {
                let offset = fast.try_read::<u64>()?;
                fast.skip(8);
                let file_object = fast.try_read::<usize>()?;
                let file_key = fast.try_read::<usize>()?;
                fast.skip(4);
                let size = fast.try_read::<u32>()?;
                let flags = fast.try_read::<u32>()?;

                if cfg!(debug_assertions) {
                    debug_parser!(parser, record, schema_locator);
                    assert_eq!(
                        offset,
                        parser
                            .try_parse::<u64>("Offset")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        file_object,
                        *parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        file_key,
                        *parser
                            .try_parse::<Pointer>("FileKey")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        size,
                        parser
                            .try_parse::<u32>("IoSize")
                            .map_err(RuntimeError::from)?
                    );
                    assert_eq!(
                        flags,
                        parser
                            .try_parse::<u32>("IoFlags")
                            .map_err(RuntimeError::from)?
                    );
                }

                match self._mapping.try_lock() {
                    Some(mut mapping) => match mapping.get(&file_key).cloned() {
                        Some(file_path) => Ok(Some(Event::new(
                            record,
                            EventData::FileReadWrite {
                                offset,
                                file_object,
                                size,
                                flags,
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
}

impl KernelProviderWrapper for FileProviderWrapper {
    fn provider(&self) -> &KernelProvider {
        &Self::_PROVIDER
    }
}
