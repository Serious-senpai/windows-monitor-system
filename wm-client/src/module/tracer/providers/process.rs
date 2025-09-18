use std::error::Error;
use std::sync::Arc;

use ferrisetw::parser::{Parser, Pointer};
use ferrisetw::provider::kernel_providers::{KernelProvider, PROCESS_PROVIDER};
use ferrisetw::{EventRecord, SchemaLocator};
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::ProviderWrapper;

pub struct ProcessProviderWrapper;

impl ProviderWrapper for ProcessProviderWrapper {
    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &PROCESS_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.opcode() == 1 || record.opcode() == 2
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Event, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);
                // let unique_process_key = parser
                //     .try_parse::<Pointer>("UniqueProcessKey")
                //     .map_err(RuntimeError::from)?;
                let process_id = parser
                    .try_parse::<u32>("ProcessId")
                    .map_err(RuntimeError::from)?;
                // let parent_id = parser
                //     .try_parse::<u32>("ParentId")
                //     .map_err(RuntimeError::from)?;
                // let session_id = parser
                //     .try_parse::<u32>("SessionId")
                //     .map_err(RuntimeError::from)?;
                let exit_status = parser
                    .try_parse::<i32>("ExitStatus")
                    .map_err(RuntimeError::from)?;
                // let directory_table_base = parser
                //     .try_parse::<Pointer>("DirectoryTableBase")
                //     .map_err(RuntimeError::from)?;
                let image_file_name = parser
                    .try_parse::<String>("ImageFileName")
                    .map_err(RuntimeError::from)?;
                let command_line = parser
                    .try_parse::<String>("CommandLine")
                    .map_err(RuntimeError::from)?;

                Ok(Event::new(
                    record,
                    EventData::Process {
                        // unique_process_key: *unique_process_key,
                        process_id,
                        // parent_id,
                        // session_id,
                        exit_status,
                        // directory_table_base: *directory_table_base,
                        image_file_name,
                        command_line,
                    },
                ))
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}
