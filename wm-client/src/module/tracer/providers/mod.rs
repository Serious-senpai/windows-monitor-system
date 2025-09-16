pub mod file;
pub mod image;
pub mod process;
pub mod registry;
pub mod tcpip;
pub mod udpip;

use std::error::Error;
use std::sync::Arc;

use chrono::Utc;
use ferrisetw::provider::Provider;
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::trace::{KernelTrace, TraceBuilder};
use ferrisetw::{EventRecord, SchemaLocator};
use log::{debug, error, warn};
use parking_lot::Mutex as BlockingMutex;
use tokio::sync::{Mutex, mpsc};
use wm_common::schema::event::{CapturedEventRecord, Event};

use crate::backup::Backup;
use crate::module::tracer::enricher::BlockingEventEnricher;

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
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>>;

    fn attach(
        self: Arc<Self>,
        trace: TraceBuilder<KernelTrace>,
        sender: mpsc::Sender<Arc<CapturedEventRecord>>,
        enricher: Arc<BlockingMutex<BlockingEventEnricher>>,
        backup: Arc<Mutex<Backup>>,
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
                    // cargo fmt error here: https://github.com/rust-lang/rustfmt/issues/5689
                    match ptr.clone().callback(record, schema_locator) {
                        Ok(Some(event)) => match enricher.try_lock() {
                            Some(mut enricher) => {
                                let data = Arc::new(CapturedEventRecord {
                                    event,
                                    system: enricher.system.system_info(),
                                    captured: Utc::now(),
                                });

                                if sender.try_send(data.clone()).is_err() {
                                    warn!(
                                        "Message queue is full, backing up event to persistent file"
                                    );

                                    let backup = backup.clone();
                                    tokio::spawn(async move {
                                        let mut backup = backup.lock().await;
                                        backup.write_one(&data).await;
                                    });
                                }
                            }
                            None => {
                                error!(
                                    "Inconsistent state reached. This mutex should never block."
                                );
                            }
                        },
                        Ok(None) => {}
                        Err(e) => {
                            error!("Error handling event from {:?}: {e}", provider.guid);
                        }
                    }
                }
            })
            .build();

        trace.enable(provider)
    }
}
