use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::{EventRecord, GUID, SchemaLocator};
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::{ProviderWrapper, UserProviderWrapper};

pub struct FileProviderWrapper;

impl FileProviderWrapper {
    const _GUID: GUID = GUID::from_values(
        0xedd08927,
        0x9cc4,
        0x4e65,
        [0xb9, 0x70, 0xc2, 0x56, 0x0f, 0xb5, 0xc2, 0x89],
    );
}

impl ProviderWrapper for FileProviderWrapper {
    fn filter(&self, record: &EventRecord) -> bool {
        record.opcode() == 10
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);
                let file_object = parser
                    .try_parse::<Pointer>("FileKey")
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
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}

impl UserProviderWrapper for FileProviderWrapper {
    fn guid(&self) -> &GUID {
        &Self::_GUID
    }
}
