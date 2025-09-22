pub mod kernel;
pub mod user;

use std::error::Error;
use std::sync::Arc;

use chrono::Utc;
use ferrisetw::provider::Provider;
use ferrisetw::provider::kernel_providers::KernelProvider;
use ferrisetw::trace::{KernelTrace, TraceBuilder};
use ferrisetw::{EventRecord, GUID, SchemaLocator, UserTrace};
use log::{debug, error, warn};
use parking_lot::Mutex as BlockingMutex;
use tokio::sync::{Mutex, mpsc};
use wm_common::schema::event::{CapturedEventRecord, Event};

use crate::backup::Backup;
use crate::module::tracer::enricher::BlockingEventEnricher;

pub trait ProviderWrapper: Send + Sync {
    fn filter(&self, record: &EventRecord) -> bool;

    fn callback(
        self: Arc<Self>,
        record: &EventRecord,
        schema_locator: &SchemaLocator,
    ) -> Result<Option<Event>, Box<dyn Error + Send + Sync>>;
}

fn _callback_impl<T>(
    wrapper: Arc<T>,
    record: &EventRecord,
    schema_locator: &SchemaLocator,
    sender: mpsc::Sender<Arc<CapturedEventRecord>>,
    enricher: Arc<BlockingMutex<BlockingEventEnricher>>,
    backup: Arc<Mutex<Backup>>,
) where
    T: ProviderWrapper + ?Sized,
{
    if wrapper.filter(record) {
        // cargo fmt error here: https://github.com/rust-lang/rustfmt/issues/5689
        match wrapper.clone().callback(record, schema_locator) {
            Ok(Some(event)) => match enricher.try_lock() {
                Some(mut enricher) => {
                    let data = Arc::new(CapturedEventRecord {
                        event,
                        system: enricher.system.system_info(),
                        captured: Utc::now(),
                    });

                    if sender.try_send(data.clone()).is_err() {
                        warn!("Message queue is full, backing up event to persistent file");

                        let backup = backup.clone();
                        tokio::spawn(async move {
                            let mut backup = backup.lock().await;
                            backup.write_one(&data).await;
                        });
                    }
                }
                None => {
                    error!("Inconsistent state reached. This mutex should never block.");
                }
            },
            Ok(None) => {}
            Err(e) => error!(
                "Error handling event from {:?} (event_id={}, opcode={}, version={}, level={}, keyword={}, pid={}, tid={}): {e}",
                record.provider_id(),
                record.event_id(),
                record.opcode(),
                record.version(),
                record.level(),
                record.keyword(),
                record.process_id(),
                record.thread_id(),
            ),
        }
    }
}

pub trait KernelProviderWrapper: ProviderWrapper {
    fn provider(&self) -> &KernelProvider;

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
        let provider = self.provider();
        debug!("Attaching kernel provider {:?}", provider.guid);

        let provider = Provider::kernel(provider)
            .add_callback(move |record, schema_locator| {
                _callback_impl(
                    self.clone(),
                    record,
                    schema_locator,
                    sender.clone(),
                    enricher.clone(),
                    backup.clone(),
                );
            })
            .build();

        trace.enable(provider)
    }
}

pub trait UserProviderWrapper: ProviderWrapper {
    fn guid(&self) -> &GUID;

    fn attach(
        self: Arc<Self>,
        trace: TraceBuilder<UserTrace>,
        sender: mpsc::Sender<Arc<CapturedEventRecord>>,
        enricher: Arc<BlockingMutex<BlockingEventEnricher>>,
        backup: Arc<Mutex<Backup>>,
    ) -> TraceBuilder<UserTrace>
    where
        Self: 'static,
    {
        let guid = self.guid();
        debug!("Attaching user provider {guid:?}");

        let provider = Provider::by_guid(*guid)
            .add_callback(move |record, schema_locator| {
                _callback_impl(
                    self.clone(),
                    record,
                    schema_locator,
                    sender.clone(),
                    enricher.clone(),
                    backup.clone(),
                );
            })
            .build();

        trace.enable(provider)
    }
}
