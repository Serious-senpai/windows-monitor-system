use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::{KernelProvider, PROCESS_PROVIDER};
use ferrisetw::{EventRecord, SchemaLocator};
use windows::Win32::Security::TOKEN_USER;
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::debug_parser;
use crate::module::tracer::providers::fast_parser::FastParser;
use crate::module::tracer::providers::{KernelProviderWrapper, ProviderWrapper};

pub struct ProcessProviderWrapper;

impl ProviderWrapper for ProcessProviderWrapper {
    fn filter(&self, record: &EventRecord) -> bool {
        record.opcode() == 1 || record.opcode() == 2
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>> {
        let mut fast = FastParser::new(record);
        let unique_process_key = fast.try_read::<usize>()?;
        let process_id = fast.try_read::<u32>()?;
        let parent_id = fast.try_read::<u32>()?;
        let session_id = fast.try_read::<u32>()?;
        let exit_status = fast.try_read::<i32>()?;
        let directory_table_base = fast.try_read::<usize>()?;

        fast.skip(4 + size_of::<TOKEN_USER>() + 1);
        let count = fast.try_read::<u8>()?;
        fast.skip(6 + count as usize * 4);

        let image_file_name = fast.try_read_utf8()?;
        let command_line = fast.try_read_utf16()?;

        if cfg!(debug_assertions) {
            debug_parser!(parser, record, schema_locator);
            assert_eq!(
                unique_process_key,
                *parser
                    .try_parse::<Pointer>("UniqueProcessKey")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                process_id,
                parser
                    .try_parse::<u32>("ProcessId")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                parent_id,
                parser
                    .try_parse::<u32>("ParentId")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                session_id,
                parser
                    .try_parse::<u32>("SessionId")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                exit_status,
                parser
                    .try_parse::<i32>("ExitStatus")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                directory_table_base,
                *parser
                    .try_parse::<Pointer>("DirectoryTableBase")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                image_file_name,
                parser
                    .try_parse::<String>("ImageFileName")
                    .map_err(RuntimeError::from)?
            );
            assert_eq!(
                command_line,
                parser
                    .try_parse::<String>("CommandLine")
                    .map_err(RuntimeError::from)?
            );
        }

        Ok(Some(Event::new(
            record,
            EventData::Process {
                unique_process_key,
                process_id,
                parent_id,
                session_id,
                exit_status,
                directory_table_base,
                image_file_name,
                command_line,
            },
        )))
    }
}

impl KernelProviderWrapper for ProcessProviderWrapper {
    fn provider(&self) -> &'static KernelProvider {
        &PROCESS_PROVIDER
    }
}
