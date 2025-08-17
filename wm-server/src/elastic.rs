use std::env;
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
use tokio::sync::OnceCell;

use crate::models::users::User;
use crate::utils;

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

async fn _initialize() -> Result<ElasticsearchWrapper, Box<dyn Error + Send + Sync>> {
    let transport = Transport::single_node(env::var("ELASTIC_HOST")?.as_str())?;
    transport.set_auth(Credentials::Basic(
        env::var("ELASTIC_USERNAME")?,
        env::var("ELASTIC_PASSWORD")?,
    ));
    let elastic = ElasticsearchWrapper {
        client: Elasticsearch::new(transport),
    };

    let _ = _create_document(
        &elastic.client,
        "users.windows-monitor",
        &env::var("ELASTIC_USERNAME")?,
        &User {
            username: env::var("ELASTIC_USERNAME")?,
            hashed_password: utils::hash_password(&env::var("ELASTIC_PASSWORD")?, None),
            permission: 1,
        },
    )
    .await;

    _put_index_template(
        &elastic.client,
        "events.windows-monitor",
        serde_json::from_str::<serde_json::Value>(include_str!("indices/ecs-template.json"))?,
    )
    .await?;

    Ok(elastic)
}

pub struct ElasticsearchWrapper {
    pub client: Elasticsearch,
}

impl ElasticsearchWrapper {
    pub async fn singleton() -> Result<&'static Self, Box<dyn Error + Send + Sync>> {
        static INSTANCE: OnceCell<ElasticsearchWrapper> = OnceCell::const_new();
        INSTANCE.get_or_try_init(_initialize).await
    }
}
