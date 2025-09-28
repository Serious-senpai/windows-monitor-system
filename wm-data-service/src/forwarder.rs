use std::mem;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Weak};

use elasticsearch::BulkParts;
use lapin::acker::Acker;
use lapin::message::Delivery;
use lapin::options::{BasicAckOptions, BasicNackOptions};
use log::{debug, error};
use wm_common::schema::event::CapturedEventRecord;

use crate::app::App;

/// Message forwarder transforms messages coming from RabbitMQ, construct
/// an appropriate HTTP request and send it to Elasticsearch HTTP API.
pub struct MessageForwarder {
    _app: Weak<App>,
    _body: Vec<u8>,
    _acker: Option<Acker>,
}

impl MessageForwarder {
    pub fn new(app: &Arc<App>) -> Self {
        Self {
            _app: Arc::downgrade(app),
            _body: Vec::with_capacity(app.config().throughput.flush_limit * 3 / 2),
            _acker: None,
        }
    }

    async fn _ack(&mut self) {
        if let Some(acker) = self._acker.take() {
            debug!("Sending ACK to RabbitMQ");
            if let Err(e) = acker.ack(BasicAckOptions { multiple: true }).await {
                error!("Failed to send ACK to RabbitMQ: {e}");
            }
        }
    }

    async fn _nack(&mut self) {
        if let Some(acker) = self._acker.take() {
            debug!("Sending NACK to RabbitMQ");
            if let Err(e) = acker
                .nack(BasicNackOptions {
                    multiple: true,
                    requeue: true,
                })
                .await
            {
                error!("Failed to send NACK to RabbitMQ: {e}");
            }
        }
    }

    pub async fn process(&mut self, delivery: Option<Delivery>) {
        if let Some(app) = self._app.upgrade() {
            let push_to_elastic = if let Some(delivery) = delivery {
                let Delivery {
                    mut data, acker, ..
                } = delivery;
                self._acker = Some(acker);

                match data.pop() {
                    Some(is_ipv4) => {
                        let ip_native_order = u128::from_be_bytes(
                            data[data.len() - 16..]
                                .try_into()
                                .expect("Slice does not have 16 bytes"),
                        );
                        data.truncate(data.len() - 16);
                        let ip = if is_ipv4 != 0 {
                            IpAddr::V4(Ipv4Addr::from(
                                u32::try_from(ip_native_order & 0xFFFFFFFF)
                                    .expect("Cannot convert to u32"),
                            ))
                        } else {
                            IpAddr::V6(Ipv6Addr::from(ip_native_order))
                        };

                        match serde_json::from_slice::<CapturedEventRecord>(&data) {
                            Ok(event) => {
                                self._body.extend_from_slice(b"{\"create\":{}}\n");

                                let ecs = event.to_ecs(ip);
                                serde_json::to_writer(&mut self._body, &ecs).unwrap();
                                self._body.push(b'\n');

                                self._body.len() >= app.config().throughput.flush_limit
                            }
                            Err(e) => {
                                error!("Invalid event JSON: {e}");
                                false
                            }
                        }
                    }
                    None => false,
                }
            } else {
                // Push to Elasticsearch on timeout
                true
            };

            if push_to_elastic && !self._body.is_empty() {
                let app = app.clone();

                let mut moved_body = Vec::with_capacity(self._body.capacity());
                mem::swap(&mut moved_body, &mut self._body);

                match app.elastic().await {
                    Some(elastic) => {
                        match elastic
                            .client()
                            .bulk(BulkParts::Index("events.windows-monitor-ecs"))
                            .body(vec![moved_body])
                            .send()
                            .await
                        {
                            Ok(_) => {
                                self._ack().await;
                            }
                            Err(e) => {
                                error!("Elasticsearch API error: {e}");
                                self._nack().await;
                            }
                        }
                    }
                    None => {
                        self._nack().await;
                    }
                }
            }
        }
    }
}
