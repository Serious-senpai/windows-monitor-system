use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::{KernelProvider, REGISTRY_PROVIDER};
use ferrisetw::{EventRecord, SchemaLocator};
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::ProviderWrapper;

pub struct RegistryProviderWrapper;

impl ProviderWrapper for RegistryProviderWrapper {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
    }

    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &REGISTRY_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.opcode() == 10
            || record.opcode() == 12
            || record.opcode() == 14
            || record.opcode() == 15
            || record.opcode() == 20
            || record.opcode() == 21
            || record.opcode() == 22
            || record.opcode() == 23
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);
                let initial_time = parser
                    .try_parse::<i64>("InitialTime")
                    .map_err(RuntimeError::from)?;
                let status = parser
                    .try_parse::<Pointer>("Status")
                    .map_err(RuntimeError::from)?;
                let index = parser
                    .try_parse::<u32>("Index")
                    .map_err(RuntimeError::from)?;
                let key_handle = parser
                    .try_parse::<Pointer>("KeyHandle")
                    .map_err(RuntimeError::from)?;
                let key_name = parser
                    .try_parse::<String>("KeyName")
                    .map_err(RuntimeError::from)?;

                Ok(Some(Event::new(
                    record,
                    EventData::Registry {
                        initial_time,
                        status: *status,
                        index,
                        key_handle: *key_handle,
                        key_name: key_name.clone(),
                    },
                )))
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}
