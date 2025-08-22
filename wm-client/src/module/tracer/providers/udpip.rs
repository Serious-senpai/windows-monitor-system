use std::error::Error;
use std::net::IpAddr;
use std::sync::Arc;

use ferrisetw::parser::Parser;
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::{EventRecord, GUID, SchemaLocator};
use windows::Win32::System::Diagnostics::Etw::EVENT_TRACE_FLAG_NETWORK_TCPIP;
use wm_common::error::RuntimeError;
use wm_common::schema::event::{Event, EventData};

use crate::module::tracer::providers::ProviderWrapper;

const _PROVIDER: KernelProvider = KernelProvider::new(
    GUID::from_values(
        0xbf3a50c5,
        0xa9c9,
        0x4988,
        [0xa0, 0x05, 0x2d, 0xf0, 0xb7, 0xc8, 0x0f, 0x80],
    ),
    EVENT_TRACE_FLAG_NETWORK_TCPIP.0,
);

pub struct UdpIpProviderWrapper;

impl ProviderWrapper for UdpIpProviderWrapper {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
    }

    fn provider(self: Arc<Self>) -> &'static KernelProvider {
        &_PROVIDER
    }

    fn filter(self: Arc<Self>, record: &EventRecord) -> bool {
        record.opcode() == 10 || record.opcode() == 11
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
                    raw_timestamp: record.raw_timestamp(),
                    process_id: pid,
                    thread_id: record.thread_id(),
                    event_id: record.event_id(),
                    opcode: record.opcode(),
                    data: EventData::UdpIp {
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
