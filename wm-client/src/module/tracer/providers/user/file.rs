use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::{EventRecord, GUID, SchemaLocator};
use lru::LruCache;
use parking_lot::Mutex as BlockingMutex;
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::{ProviderWrapper, UserProviderWrapper};

pub struct FileProviderWrapper {
    _mapping: BlockingMutex<LruCache<usize, String>>,
}

impl FileProviderWrapper {
    const _GUID: GUID = GUID::from_values(
        0xedd08927,
        0x9cc4,
        0x4e65,
        [0xb9, 0x70, 0xc2, 0x56, 0x0f, 0xb5, 0xc2, 0x89],
    );

    const _MAP_SIZE: usize = 1000;

    pub fn new() -> Self {
        Self {
            _mapping: BlockingMutex::new(LruCache::new(
                NonZeroUsize::new(Self::_MAP_SIZE)
                    .unwrap_or_else(|| panic!("{} > 0", Self::_MAP_SIZE)),
            )),
        }
    }
}

impl ProviderWrapper for FileProviderWrapper {
    fn filter(&self, record: &EventRecord) -> bool {
        record.event_id() == 10 || record.event_id() == 11 || record.event_id() == 12
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);

                match record.event_id() {
                    10 | 11 => {
                        let file_key = parser
                            .try_parse::<Pointer>("FileKey")
                            .map_err(RuntimeError::from)?;
                        let file_name = parser
                            .try_parse::<String>("FileName")
                            .map_err(RuntimeError::from)?;

                        match self._mapping.try_lock() {
                            Some(mut mapping) => {
                                mapping.put(*file_key, file_name.clone());
                            }
                            None => panic!("This mutex should never block."),
                        }

                        Ok(Some(Event::new(
                            record,
                            EventData::File {
                                file_object: *file_key,
                                file_name,
                            },
                        )))
                    }
                    12 => {
                        let file_object = parser
                            .try_parse::<Pointer>("FileObject")
                            .map_err(RuntimeError::from)?;
                        let file_name = parser
                            .try_parse::<String>("FileName")
                            .map_err(RuntimeError::from)?;

                        Ok(Some(Event::new(
                            record,
                            EventData::File {
                                file_object: *file_object,
                                file_name,
                            },
                        )))
                    }
                    other => Err(RuntimeError::new(format!("Unexpected event_id {other}")))?,
                }
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}

impl UserProviderWrapper for FileProviderWrapper {
    fn guid(&self) -> &GUID {
        &Self::_GUID
    }
}
