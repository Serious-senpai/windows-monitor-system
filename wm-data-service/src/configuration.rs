use serde::{Deserialize, Serialize};
use url::Url;
use wm_common::logger::LogLevel;

#[derive(Deserialize, Serialize)]
pub struct EventPostSettings {
    pub flush_limit: usize,
}

#[derive(Deserialize, Serialize)]
pub struct RabbitMQ {
    pub host: Url,
}

#[derive(Deserialize, Serialize)]
pub struct Elasticsearch {
    pub host: Url,
    pub kibana: Url,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub log_level: LogLevel,
    pub event_post: EventPostSettings,
    pub rabbitmq: RabbitMQ,
    pub elasticsearch: Elasticsearch,
}
