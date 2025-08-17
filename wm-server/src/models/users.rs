use std::error::Error;
use std::io;

use elasticsearch::SearchParts;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::elastic::ElasticsearchWrapper;
use crate::utils;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub username: String,
    pub hashed_password: String,
    pub permission: i64,
}

impl User {
    pub async fn query(username: &str) -> Result<Option<Self>, Box<dyn Error + Send + Sync>> {
        let elastic = ElasticsearchWrapper::singleton().await?;
        let response = elastic
            .client
            .search(SearchParts::Index(&["users.windows-monitor"]))
            .body(json!({
                "query": {
                    "term": {
                        "username": username
                    }
                }
            }))
            .send()
            .await?;

        let response_body = response.json::<Value>().await?;

        let hits = response_body["hits"]["hits"]
            .as_array()
            .ok_or_else(|| io::Error::other("Invalid response from Elasticsearch"))?;

        if hits.len() == 1 {
            let user = serde_json::from_value(hits[0]["_source"].clone())?;
            Ok(Some(user))
        } else if hits.is_empty() {
            Ok(None)
        } else {
            Err(io::Error::other(
                "Multiple users found with the same username",
            ))?
        }
    }

    pub async fn create(
        username: &str,
        password: &str,
        permission: i64,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let user = Self {
            username: username.to_string(),
            hashed_password: utils::hash_password(password, None),
            permission,
        };

        let elastic = ElasticsearchWrapper::singleton().await?;
        let response = elastic
            .client
            .index(elasticsearch::IndexParts::IndexId(
                "users.windows-monitor",
                username,
            )) // Using `username` as ID enforces uniqueness
            .body(user.clone())
            .send()
            .await?;

        if response.status_code().is_success() {
            Ok(user)
        } else {
            let text = response.text().await?;
            Err(io::Error::other(text))?
        }
    }
}
