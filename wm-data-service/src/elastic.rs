use std::error::Error;
use std::sync::Arc;

use elasticsearch::Elasticsearch;
use elasticsearch::auth::Credentials;
use elasticsearch::http::response::Response;
use elasticsearch::http::transport::Transport;
use elasticsearch::indices::IndicesPutIndexTemplateParts;
use log::{debug, warn};

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

pub struct KibanaClient {
    _config: Arc<Configuration>,
    _http: reqwest::Client,
}

impl KibanaClient {
    pub fn new(config: Arc<Configuration>) -> Self {
        Self {
            _config: config,
            _http: reqwest::Client::new(),
        }
    }

    pub fn get(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::GET, endpoint)
    }

    pub fn post(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::POST, endpoint)
    }

    pub fn put(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::PUT, endpoint)
    }

    pub fn patch(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::PATCH, endpoint)
    }

    pub fn delete(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::DELETE, endpoint)
    }

    pub fn head(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::HEAD, endpoint)
    }

    pub fn request(&self, method: reqwest::Method, endpoint: &str) -> reqwest::RequestBuilder {
        let url = self
            ._config
            .elasticsearch
            .kibana
            .join(endpoint)
            .unwrap_or_else(|_| panic!("Failed to construct URL to {endpoint}"));

        self._http.request(method, url).basic_auth(
            &self._config.elasticsearch.username,
            Some(&self._config.elasticsearch.password),
        )
    }
}

pub struct ElasticsearchWrapper {
    _client: Elasticsearch,
    _kibana: KibanaClient,
}

impl ElasticsearchWrapper {
    pub async fn async_new(
        config: Arc<Configuration>,
    ) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
        let transport = Transport::single_node(config.elasticsearch.host.as_str())?;
        transport.set_auth(Credentials::Basic(
            config.elasticsearch.username.clone(),
            config.elasticsearch.password.clone(),
        ));
        let elastic = Self {
            _client: Elasticsearch::new(transport),
            _kibana: KibanaClient::new(config.clone()),
        };

        let response = elastic
            ._client
            .indices()
            .put_index_template(IndicesPutIndexTemplateParts::Name(
                "events.windows-monitor-ecs",
            ))
            .body(serde_json::from_str::<serde_json::Value>(include_str!(
                "../../services/elastic/ecs-template.json"
            ))?)
            .create(true)
            .send()
            .await?;
        _log_error(response).await;

        Ok(Arc::new(elastic))
    }

    pub fn client(&self) -> &Elasticsearch {
        &self._client
    }

    pub fn kibana(&self) -> &KibanaClient {
        &self._kibana
    }
}
