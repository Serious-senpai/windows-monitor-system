use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::{IMAGE_LOAD_PROVIDER, KernelProvider};
use ferrisetw::{EventRecord, SchemaLocator};
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::ProviderWrapper;

pub struct ImageProviderWrapper;

impl ProviderWrapper for ImageProviderWrapper {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
    }

    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &IMAGE_LOAD_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.opcode() == 2 || record.opcode() == 10
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Event, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);
                let image_base = parser
                    .try_parse::<Pointer>("ImageBase")
                    .map_err(RuntimeError::from)?;
                let image_size = parser
                    .try_parse::<Pointer>("ImageSize")
                    .map_err(RuntimeError::from)?;
                let process_id = parser
                    .try_parse::<u32>("ProcessId")
                    .map_err(RuntimeError::from)?;
                let image_checksum = parser
                    .try_parse::<u32>("ImageChecksum")
                    .map_err(RuntimeError::from)?;
                let file_name = parser
                    .try_parse::<String>("FileName")
                    .map_err(RuntimeError::from)?;

                Ok(Event {
                    guid: format!("{:?}", record.provider_id()),
                    raw_timestamp: record.raw_timestamp(),
                    process_id,
                    thread_id: record.thread_id(),
                    event_id: record.event_id(),
                    opcode: record.opcode(),
                    data: EventData::Image {
                        image_base: *image_base,
                        image_size: *image_size,
                        process_id,
                        image_checksum,
                        file_name,
                    },
                })
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}
