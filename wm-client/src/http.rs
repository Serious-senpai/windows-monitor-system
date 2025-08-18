use std::net::SocketAddr;
use std::time::Duration;

use reqwest::Certificate;

use crate::configuration::Configuration;

#[derive(Debug)]
pub struct HttpClient {
    _client: reqwest::Client,
}

impl HttpClient {
    pub fn new(configuration: &Configuration) -> Self {
        let mut builder = reqwest::Client::builder()
            .add_root_certificate(
                Certificate::from_pem(include_bytes!("../../cert/server.pem"))
                    .expect("Failed to load server certificate"),
            )
            .timeout(Duration::from_secs(10));

        for (domain, ip) in &configuration.dns_resolver {
            builder = builder.resolve(domain, SocketAddr::new(*ip, 0));
        }

        Self {
            _client: builder.build().expect("Failed to create HTTP client"),
        }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self._client
    }
}
