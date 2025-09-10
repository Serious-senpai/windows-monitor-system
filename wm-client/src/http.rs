use std::net::SocketAddr;
use std::time::Duration;

use reqwest::{Certificate, Identity};
use url::Url;

use crate::configuration::Configuration;

#[derive(Debug)]
pub struct ApiClient {
    _base_url: Url,
    _client: reqwest::Client,
}

impl ApiClient {
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
            ._base_url
            .join(endpoint)
            .unwrap_or_else(|_| panic!("Failed to construct URL to {endpoint}"));
        self._client.request(method, url)
    }
}

#[derive(Debug)]
pub struct HttpClient {
    _api: ApiClient,
    _client: reqwest::Client,
}

impl HttpClient {
    const fn _client_certificate() -> &'static [u8] {
        include_bytes!(concat!(env!("OUT_DIR"), "/client.pfx"))
    }

    pub fn new(configuration: &Configuration, password: &str) -> Self {
        let mut builder = reqwest::Client::builder()
            .add_root_certificate(
                Certificate::from_pem(include_bytes!("../../cert/server.pem"))
                    .expect("Failed to load server certificate"),
            )
            .identity(
                Identity::from_pkcs12_der(Self::_client_certificate(), password)
                    .expect("Failed to load client identity"),
            )
            .connect_timeout(Duration::from_secs(3));

        for (domain, ip) in &configuration.dns_resolver {
            builder = builder.resolve(domain, SocketAddr::new(*ip, 0));
        }

        let client = builder.build().expect("Failed to create HTTP client");

        Self {
            _api: ApiClient {
                _base_url: configuration.server.clone(),
                _client: client.clone(),
            },
            _client: client,
        }
    }

    pub fn api(&self) -> &ApiClient {
        &self._api
    }

    pub fn client(&self) -> &reqwest::Client {
        &self._client
    }
}
