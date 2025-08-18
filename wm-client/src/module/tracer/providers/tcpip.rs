use std::error::Error;
use std::net::IpAddr;
use std::sync::Arc;

use ferrisetw::parser::Parser;
use ferrisetw::provider::kernel_providers::{KernelProvider, TCP_IP_PROVIDER};
use ferrisetw::{EventRecord, SchemaLocator};
use wm_common::error::RuntimeError;

use crate::module::tracer::data::{Event, EventData};
use crate::module::tracer::providers::ProviderWrapper;

pub struct TcpIpProviderWrapper;

impl ProviderWrapper for TcpIpProviderWrapper {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
    }

    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &TCP_IP_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.opcode() == 12 || record.opcode() == 13 || record.opcode() == 15
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Event, Box<dyn Error + Send + Sync>> {
        match schema_locator.event_schema(record) {
            Ok(schema) => {
                let parser = Parser::create(record, &schema);
                let pid = parser.try_parse::<u32>("PID").map_err(RuntimeError::from)?;
                let size = parser
                    .try_parse::<u32>("size")
                    .map_err(RuntimeError::from)?;
                let daddr = parser
                    .try_parse::<IpAddr>("daddr")
                    .map_err(RuntimeError::from)?;
                let saddr = parser
                    .try_parse::<IpAddr>("saddr")
                    .map_err(RuntimeError::from)?;
                let dport = parser
                    .try_parse::<u16>("dport")
                    .map_err(RuntimeError::from)?;
                let sport = parser
                    .try_parse::<u16>("sport")
                    .map_err(RuntimeError::from)?;

                Ok(Event {
                    guid: format!("{:?}", record.provider_id()),
                    process_id: pid,
                    thread_id: record.thread_id(),
                    event_id: record.event_id(),
                    opcode: record.opcode(),
                    data: EventData::TcpIp {
                        pid,
                        size,
                        daddr,
                        saddr,
                        dport,
                        sport,
                    },
                })
            }
            Err(e) => Err(RuntimeError::new(format!("SchemaError: {e:?}")))?,
        }
    }
}
