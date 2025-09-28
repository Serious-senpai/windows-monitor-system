use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use futures_lite::stream::StreamExt;
use lapin::options::{BasicConsumeOptions, BasicQosOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use log::{error, info};
use tokio::signal;
use tokio::time::sleep;
use wm_common::once_cell_no_retry::OnceCellNoRetry;

use crate::configuration::Configuration;
use crate::elastic::ElasticsearchWrapper;
use crate::forwarder::MessageForwarder;

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

    pub fn config(&self) -> &Configuration {
        &self._config
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
            info!("Connected to RabbitMQ");

            rabbitmq
                .queue_declare(
                    "events",
                    QueueDeclareOptions {
                        passive: false,
                        durable: true,
                        exclusive: false,
                        auto_delete: false,
                        nowait: false,
                    },
                    FieldTable::default(),
                )
                .await?;
            info!("Declared events RabbitMQ queue");

            rabbitmq
                .basic_qos(
                    self._config.throughput.prefetch_count,
                    BasicQosOptions::default(),
                )
                .await?;
            info!(
                "Set RabbitMQ prefetch count to {}",
                self._config.throughput.prefetch_count
            );

            let mut consumer = rabbitmq
                .basic_consume(
                    "events",
                    "data-service-consumer",
                    BasicConsumeOptions::default(),
                    FieldTable::default(),
                )
                .await?;
            info!("Started consuming from events queue");

            let mut forwarder = MessageForwarder::new(self);
            loop {
                let delivery = tokio::select! {
                    _ = signal::ctrl_c() => {
                        info!("Received Ctrl+C signal");
                        break;
                    }
                    Some(delivery) = consumer.next() => Some(delivery),
                    _ = sleep(Duration::from_secs(1)) => None,
                };

                match delivery.transpose() {
                    Ok(delivery) => {
                        forwarder.process(delivery).await;
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
