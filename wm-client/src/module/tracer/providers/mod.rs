pub mod file;
pub mod image;
pub mod process;
pub mod tcpip;
pub mod udpip;

use std::error::Error;
use std::sync::Arc;

use ferrisetw::provider::Provider;
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::trace::{KernelTrace, TraceBuilder};
use ferrisetw::{EventRecord, SchemaLocator};
use log::{debug, error};
use tokio::sync::mpsc;

use crate::module::tracer::data::Event;

pub trait ProviderWrapper: Send + Sync {
    fn new() -> Self
    where
        Self: Sized;

    fn provider(self: Arc<Self>) -> &'static KernelProvider;

    fn filter(self: Arc<Self>, _record: &EventRecord) -> bool {
        true
    }

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Event, Box<dyn Error + Send + Sync>>;

    fn attach(
        self: Arc<Self>,
        trace: TraceBuilder<KernelTrace>,
        sender: mpsc::UnboundedSender<Event>,
    ) -> TraceBuilder<KernelTrace>
    where
        Self: 'static,
    {
        let provider = self.clone().provider();
        debug!("Attaching provider: {:?}", provider.guid);

        let ptr = self.clone();
        let provider = Provider::kernel(provider)
            .add_callback(move |record, schema_locator| {
                if ptr.clone().filter(record) {
                    match ptr.clone().callback(record, schema_locator) {
                        Ok(event) => {
                            let _ = sender.send(event);
                        }
                        Err(e) => {
                            error!("Error when handling event from {:?}: {e}", provider.guid);
                        }
                    }
                }
            })
            .build();

        trace.enable(provider)
    }
}
