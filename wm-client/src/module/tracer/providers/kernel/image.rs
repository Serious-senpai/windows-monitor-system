use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::{IMAGE_LOAD_PROVIDER, KernelProvider};
use ferrisetw::{EventRecord, SchemaLocator};
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::debug_parser;
use crate::module::tracer::providers::fast_parser::FastParser;
use crate::module::tracer::providers::{KernelProviderWrapper, ProviderWrapper};

pub struct ImageProviderWrapper;

impl ProviderWrapper for ImageProviderWrapper {
    fn filter(&self, record: &EventRecord) -> bool {
        record.opcode() == 2 || record.opcode() == 10
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>> {
        let mut fast = FastParser::new(record);
        let image_base = fast.try_read::<usize>()?;
        let image_size = fast.try_read::<usize>()?;
        fast.skip(4);
        let image_checksum = fast.try_read::<u32>()?;
        fast.skip(32);
        let file_name = fast.try_read_utf16()?;

        if cfg!(debug_assertions) {
            debug_parser!(parser, record, schema_locator);
            assert_eq!(
                image_base,
                *parser
                    .try_parse::<Pointer>("ImageBase")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                image_size,
                *parser
                    .try_parse::<Pointer>("ImageSize")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                image_checksum,
                parser
                    .try_parse::<u32>("ImageChecksum")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                file_name,
                parser
                    .try_parse::<String>("FileName")
                    .map_err(RuntimeError::from)?
            );
        }

        Ok(Some(Event::new(
            record,
            EventData::Image {
                image_base,
                image_size,
                image_checksum,
                file_name,
            },
        )))
    }
}

impl KernelProviderWrapper for ImageProviderWrapper {
    fn provider(&self) -> &'static KernelProvider {
        &IMAGE_LOAD_PROVIDER
    }
}
