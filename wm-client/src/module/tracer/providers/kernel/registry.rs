use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::{KernelProvider, REGISTRY_PROVIDER};
use ferrisetw::{EventRecord, SchemaLocator};
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::debug_parser;
use crate::module::tracer::providers::fast_parser::FastParser;
use crate::module::tracer::providers::{KernelProviderWrapper, ProviderWrapper};

pub struct RegistryProviderWrapper;

impl ProviderWrapper for RegistryProviderWrapper {
    fn filter(&self, record: &EventRecord) -> bool {
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
        let mut fast = FastParser::new(record);
        let initial_time = fast.try_read::<i64>()?;
        let status = fast.try_read::<usize>()?;
        // let index = 0;
        let key_handle = fast.try_read::<usize>()?;
        let key_name = fast.try_read_utf16()?;

        if cfg!(debug_assertions) {
            debug_parser!(parser, record, schema_locator);
            assert_eq!(
                initial_time,
                parser
                    .try_parse::<i64>("InitialTime")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                status,
                *parser
                    .try_parse::<Pointer>("Status")
                    .map_err(RuntimeError::from)?
            );
            // assert_eq!(
            //     index,
            //     parser
            //         .try_parse::<u32>("Index")
            //         .map_err(RuntimeError::from)?
            // );
            assert_eq!(
                key_handle,
                *parser
                    .try_parse::<Pointer>("KeyHandle")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                key_name,
                parser
                    .try_parse::<String>("KeyName")
                    .map_err(RuntimeError::from)?
            );
        }

        Ok(Some(Event::new(
            record,
            EventData::Registry {
                initial_time,
                status,
                // index,
                key_handle,
                key_name,
            },
        )))
    }
}

impl KernelProviderWrapper for RegistryProviderWrapper {
    fn provider(&self) -> &'static KernelProvider {
        &REGISTRY_PROVIDER
    }
}
