// use std::error::Error;
// use std::net::IpAddr;
// use std::sync::Arc;

// use async_trait::async_trait;
// use heed::byteorder::LittleEndian;
// use heed::types::{U32, Unit};
// use heed::{Database, Env, EnvOpenOptions, RwTxn};
// use tokio::sync::{Mutex, SetOnce, mpsc};
// use wm_common::schema::event::{CapturedEventRecord, EventData};

// use crate::configuration::Configuration;
// use crate::module::Module;

// pub struct Scanner {
//     _config: Arc<Configuration>,
//     _sender: mpsc::Sender<Arc<CapturedEventRecord>>,
//     _receiver: Mutex<mpsc::Receiver<Arc<CapturedEventRecord>>>,
//     _env: Arc<Env>,
//     _stopped: SetOnce<()>,
// }

// impl Scanner {
//     pub fn new(
//         config: Arc<Configuration>,
//         sender: mpsc::Sender<Arc<CapturedEventRecord>>,
//         receiver: mpsc::Receiver<Arc<CapturedEventRecord>>,
//     ) -> Self
//     where
//         Self: Sized,
//     {
//         let env = unsafe {
//             Arc::new(
//                 EnvOpenOptions::new()
//                     .map_size(10 << 20)
//                     .open(&config.blacklist_lmdb)
//                     .expect("Unable to open LMDB"),
//             )
//         };

//         Self {
//             _config: config,
//             _sender: sender,
//             _receiver: Mutex::new(receiver),
//             _env: env,
//             _stopped: SetOnce::new(),
//         }
//     }

//     fn _open_transaction(&self) -> (RwTxn<'_>, Database<U32<LittleEndian>, Unit>) {
//         let transaction = self._env.write_txn().expect("Unable to create transaction");
//         let db = self
//             ._env
//             .open_database::<U32<LittleEndian>, Unit>(&transaction, None)
//             .expect("Unable to open LMDB")
//             .expect("Unnamed database not found");

//         (transaction, db)
//     }

//     fn _is_blacklist_ip(&self, _ip: &IpAddr) -> bool {
//         false
//     }
// }

// #[async_trait]
// impl Module for Scanner {
//     fn name(&self) -> &str {
//         "Scanner"
//     }

//     async fn run(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
//         let mut receiver = self._receiver.lock().await;
//         while self._stopped.get().is_none() {
//             let event = tokio::select! {
//                 _ = self._stopped.wait() => break,
//                 event = receiver.recv() => match event {
//                     Some(event) => event,
//                     None => break,
//                 },
//             };

//             match &event.event.data {
//                 EventData::TcpIp { daddr, .. } | EventData::UdpIp { daddr, .. } => {
//                     if self._is_blacklist_ip(daddr) {
//                         // TODO: Handle blacklisted IP
//                     }
//                 }
//                 _ => {}
//             }
//         }

//         Ok(())
//     }
//     async fn stop(self: Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
//         self._stopped.set(())?;
//         Ok(())
//     }
// }
