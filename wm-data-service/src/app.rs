use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use futures_lite::stream::StreamExt;
use lapin::options::BasicConsumeOptions;
use lapin::types::FieldTable;
use log::{error, info};
use tokio::signal;
use wm_common::once_cell_no_retry::OnceCellNoRetry;
use wm_common::schema::event::CapturedEventRecord;

use crate::configuration::Configuration;
use crate::elastic::ElasticsearchWrapper;

pub struct App {
    _config: Arc<Configuration>,
    _rabbitmq: OnceCellNoRetry<Arc<lapin::Channel>>,
    _elastic: OnceCellNoRetry<Arc<ElasticsearchWrapper>>,
}

impl App {
    async fn _initialize_rabbitmq(
        &self,
    ) -> Result<Arc<lapin::Channel>, Box<dyn Error + Send + Sync>> {
        Ok(Arc::new(
            lapin::Connection::connect(
                self._config.rabbitmq.host.as_str(),
                lapin::ConnectionProperties::default()
                    .with_executor(tokio_executor_trait::Tokio::current()),
            )
            .await?
            .create_channel()
            .await?,
        ))
    }

    pub fn new(config: Arc<Configuration>) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
        let this = Arc::new(Self {
            _config: config,
            _rabbitmq: OnceCellNoRetry::new(),
            _elastic: OnceCellNoRetry::new(),
        });

        // Try initializing Elasticsearch connection
        let this_cloned = this.clone();
        tokio::spawn(async move {
            let _ = this_cloned.elastic().await;
        });

        // Try initializing RabbitMQ connection
        let this_cloned = this.clone();
        tokio::spawn(async move {
            let _ = this_cloned.rabbitmq().await;
        });

        Ok(this)
    }

    pub async fn rabbitmq(&self) -> Option<Arc<lapin::Channel>> {
        self._rabbitmq
            .get_or_try_init(|| async {
                self._initialize_rabbitmq().await.map_err(|e| {
                    error!("Unable to connect to RabbitMQ: {e}");
                    e
                })
            })
            .await
            .cloned()
    }

    pub async fn elastic(&self) -> Option<Arc<ElasticsearchWrapper>> {
        self._elastic
            .get_or_try_init(async || {
                ElasticsearchWrapper::async_new(self._config.clone())
                    .await
                    .map_err(|e| {
                        error!("Unable to connect to Elasticsearch: {e}");
                        e
                    })
            })
            .await
            .cloned()
    }

    pub async fn run(self: &Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let rabbitmq = tokio::select! {
            Some(rabbitmq) = self.rabbitmq() => Some(rabbitmq),
            _ = signal::ctrl_c() => {
                info!("Received Ctrl+C signal");
                None
            }
        };

        if let Some(rabbitmq) = rabbitmq {
            let mut consumer = rabbitmq
                .basic_consume(
                    "events",
                    "",
                    BasicConsumeOptions::default(),
                    FieldTable::default(),
                )
                .await?;

            let mut body = vec![];
            loop {
                let delivery = tokio::select! {
                    _ = signal::ctrl_c() => {
                        info!("Received Ctrl+C signal");
                        break;
                    }
                    Some(delivery) = consumer.next() => delivery,
                };

                match delivery {
                    Ok(delivery) => {
                        let mut data = delivery.data;
                        if let Some(is_ipv4) = data.pop() {
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

                            if let Ok(event) = serde_json::from_slice::<CapturedEventRecord>(&data)
                            {
                                body.extend_from_slice(b"{\"create\":{}}\n");

                                let ecs = event.to_ecs(ip);
                                serde_json::to_writer(&mut body, &ecs).unwrap();
                                body.push(b'\n');
                            }
                        }
                    }
                    Err(e) => {
                        error!("RabbitMQ error: {e}");
                    }
                }
            }
        }

        Ok(())
    }
}
