pub mod file;
pub mod image;
pub mod process;
pub mod tcpip;
pub mod udpip;

use std::error::Error;
use std::sync::{Arc, RwLock as BlockingRwLock};

use ferrisetw::provider::Provider;
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::trace::{KernelTrace, TraceBuilder};
use ferrisetw::{EventRecord, SchemaLocator};
use log::{debug, error, warn};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, mpsc};
use wm_common::schema::event::{CapturedEventRecord, Event};

use crate::module::tracer::enricher::BlockingSystemInfo;

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
        sender: mpsc::Sender<Arc<CapturedEventRecord>>,
        enricher: Arc<BlockingRwLock<BlockingSystemInfo>>,
        backup: Arc<Mutex<fs::File>>,
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
                        Ok(event) => match enricher.try_write() {
                            Ok(mut enricher) => {
                                let data = Arc::new(CapturedEventRecord {
                                    event,
                                    system: enricher.system_info(),
                                });

                                if sender.try_send(data.clone()).is_err() {
                                    warn!(
                                        "Message queue is full, backing up event to persistent file"
                                    );

                                    let backup = backup.clone();
                                    tokio::spawn(async move {
                                        let mut file = backup.lock().await;
                                        let _ = file.write_u8(b'[').await;
                                        if let Err(e) = file
                                            .write_all(
                                                serde_json::to_string(&data).unwrap().as_bytes(),
                                            )
                                            .await
                                        {
                                            error!("Unable to backup. Event is lost: {e}");
                                        }

                                        let _ = file.write_u8(b']').await;
                                        let _ = file.write_u8(b'\n').await;
                                    });
                                }
                            }
                            Err(e) => {
                                error!("Unable to get event enricher. Event is lost: {e}");
                            }
                        },
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
