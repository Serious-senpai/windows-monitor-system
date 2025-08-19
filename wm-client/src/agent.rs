use std::io::{self, Write};
use std::sync::Arc;

use log::{debug, error, info};
use tokio::task;

use crate::configuration::Configuration;
use crate::http::HttpClient;
use crate::module::Module;
use crate::module::tracer::EventTracer;

pub struct Agent {
    _config: Arc<Configuration>,
    _modules: Vec<Arc<dyn Module>>,
    _http: Arc<HttpClient>,
}

impl Agent {
    pub fn new(config: Arc<Configuration>) -> Self {
        let http = Arc::new(HttpClient::new(&config));
        Self {
            _config: config.clone(),
            _modules: vec![Arc::new(EventTracer::new(config, http.clone()))],
            _http: http,
        }
    }

    pub async fn authenticate(&self) {
        info!("Authenticating with server {}", self._config.server);

        let (username, password) = match task::spawn_blocking(|| {
            let mut stdout = io::stdout();
            let stdin = io::stdin();

            print!("Username>");
            let _ = stdout.flush();

            let mut username = String::new();
            let _ = stdin.read_line(&mut username);
            username = username.trim().to_string();

            print!("Password (hidden)>");
            let _ = stdout.flush();

            let password = rpassword::read_password().expect("Unable to read password");
            (username, password)
        })
        .await
        {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to read credentials: {e}");
                return;
            }
        };

        let response = match self
            ._http
            .api()
            .post("/login")
            .basic_auth(username, Some(password))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to send authentication request: {e}");
                return;
            }
        };

        debug!("Authentication response: {response:?}");

        if response.status().is_success() {
            info!("HTTP {}", response.status());
        } else {
            error!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_else(|e| e.to_string())
            );
        }
    }

    pub async fn run(&self) {
        info!(
            "Starting agent with configuration: {}",
            serde_json::to_string(&self._config).unwrap()
        );

        let mut tasks = vec![];
        for module in &self._modules {
            let ptr = module.clone();

            let task = tokio::spawn(async move {
                if let Err(e) = ptr.clone().run().await {
                    error!("Module {} completed with error: {e}", ptr.name());
                }
            });

            tasks.push(task);
        }

        for task in tasks {
            if let Err(e) = task.await {
                error!("Task failed with error: {e}");
            }
        }

        info!("Agent run completed");
    }

    pub async fn stop(&self) {
        for module in &self._modules {
            if let Err(e) = module.clone().stop().await {
                error!("Module {} stopped with error: {e}", module.name());
            }
        }
    }
}
