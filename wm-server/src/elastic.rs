use std::error::Error;

use elasticsearch::Elasticsearch;
use elasticsearch::auth::Credentials;
use elasticsearch::http::response::Response;
use elasticsearch::http::transport::Transport;
use elasticsearch::indices::{
    IndicesCreateDataStreamParts, IndicesCreateParts, IndicesPutIndexTemplateParts,
};
use log::{debug, warn};
use serde::Serialize;

use crate::configuration::Configuration;

async fn _log_error(r: Response) -> bool {
    if r.status_code().is_success() {
        debug!("HTTP response {}", r.status_code());
        true
    } else {
        warn!("HTTP response {}", r.status_code());

        match r.text().await {
            Ok(text) => {
                warn!("{text}");
            }
            Err(e) => {
                warn!("Failed to read response body: {e}");
            }
        }

        false
    }
}

async fn _create_index(
    elastic: &Elasticsearch,
    name: &str,
    body: impl Serialize,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let response = elastic
        .indices()
        .create(IndicesCreateParts::Index(name))
        .body(body)
        .send()
        .await?;

    Ok(_log_error(response).await)
}

async fn _put_index_template(
    elastic: &Elasticsearch,
    name: &str,
    body: impl Serialize,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let response = elastic
        .indices()
        .put_index_template(IndicesPutIndexTemplateParts::Name(name))
        .body(body)
        .send()
        .await?;

    Ok(_log_error(response).await)
}

async fn _create_data_stream(
    elastic: &Elasticsearch,
    name: &str,
    body: impl Serialize,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let response = elastic
        .indices()
        .create_data_stream(IndicesCreateDataStreamParts::Name(name))
        .body(body)
        .send()
        .await?;

    Ok(_log_error(response).await)
}

async fn _create_document(
    elastic: &Elasticsearch,
    index: &str,
    id: &str,
    body: impl Serialize,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let response = elastic
        .index(elasticsearch::IndexParts::IndexId(index, id))
        .body(body)
        .send()
        .await?;

    Ok(_log_error(response).await)
}

pub struct ElasticsearchWrapper {
    _client: Elasticsearch,
}

impl ElasticsearchWrapper {
    pub async fn new(config: &Configuration) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let transport = Transport::single_node(config.elasticsearch.host.as_str())?;
        transport.set_auth(Credentials::Basic(
            config.elasticsearch.username.clone(),
            config.elasticsearch.password.clone(),
        ));
        let elastic = Self {
            _client: Elasticsearch::new(transport),
        };

        _put_index_template(
            &elastic._client,
            "events.windows-monitor",
            serde_json::from_str::<serde_json::Value>(include_str!("indices/ecs-template.json"))?,
        )
        .await?;

        Ok(elastic)
    }

    pub fn client(&self) -> &Elasticsearch {
        &self._client
    }
}
