use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use hyper::StatusCode;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use lapin::options::QueueDeclareOptions;
use lapin::types::FieldTable;
use log::{debug, error, info};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use tokio::net::TcpListener;
use tokio::{signal, task};
use tokio_rustls::TlsAcceptor;
use wm_common::once_cell_no_retry::OnceCellNoRetry;

use crate::configuration::Configuration;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;
use crate::routes::backup::BackupService;
use crate::routes::health_check::HealthCheckService;
use crate::routes::trace::TraceService;

pub struct App {
    _config: Arc<Configuration>,
    _services: HashMap<String, Arc<dyn Service>>,
    _rabbitmq: OnceCellNoRetry<Arc<lapin::Channel>>,
}

impl App {
    /// Load public certificate from file.
    fn _load_certs(filename: &PathBuf) -> io::Result<Vec<CertificateDer<'static>>> {
        // Open certificate file.
        let certfile = File::open(filename)?;
        let mut reader = io::BufReader::new(certfile);

        // Load and return certificate.
        rustls_pemfile::certs(&mut reader).collect()
    }

    /// Load private key from file.
    fn _load_private_key(filename: &PathBuf) -> io::Result<PrivateKeyDer<'static>> {
        // Open keyfile.
        let keyfile = File::open(filename)?;
        let mut reader = io::BufReader::new(keyfile);

        // Load and return a single private key.
        rustls_pemfile::private_key(&mut reader).map(|key| key.unwrap())
    }

    async fn _initialize_rabbitmq(
        &self,
    ) -> Result<Arc<lapin::Channel>, Box<dyn Error + Send + Sync>> {
        let rabbitmq = Arc::new(
            lapin::Connection::connect(
                self._config.rabbitmq.host.as_str(),
                lapin::ConnectionProperties::default()
                    .with_executor(tokio_executor_trait::Tokio::current()),
            )
            .await?
            .create_channel()
            .await?,
        );
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

        Ok(rabbitmq)
    }

    pub fn new(config: Arc<Configuration>) -> Arc<Self> {
        let mut services = HashMap::new();

        for service in [
            Arc::new(BackupService {}) as Arc<dyn Service>,
            Arc::new(HealthCheckService {}) as Arc<dyn Service>,
            Arc::new(TraceService {}) as Arc<dyn Service>,
        ] {
            services.insert(service.route().to_string(), service);
        }

        let this = Arc::new(Self {
            _config: config,
            _services: services,
            _rabbitmq: OnceCellNoRetry::new(),
        });

        // Try initializing RabbitMQ connection
        let this_cloned = this.clone();
        tokio::spawn(async move {
            let _ = this_cloned.rabbitmq().await;
        });

        this
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

    pub async fn run(self: &Arc<Self>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self._config.port));
        let certs =
            Self::_load_certs(&self._config.certificate).expect("Failed to load certificate");
        let key =
            Self::_load_private_key(&self._config.private_key).expect("Failed to load private key");

        let root_ca = webpki::anchor_from_trusted_cert(
            certs
                .last()
                .expect("There should be at least 1 certificate"),
        )
        .expect("Failed to create root CA")
        .to_owned();

        let listener = TcpListener::bind(addr).await?;
        let mut cfg = ServerConfig::builder()
            .with_client_cert_verifier(
                WebPkiClientVerifier::builder(Arc::new(RootCertStore {
                    roots: vec![root_ca],
                }))
                .build()
                .expect("Unable to create WebPkiClientVerifier"),
            )
            .with_single_cert(certs, key)?;
        cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];

        let tls = TlsAcceptor::from(Arc::new(cfg));

        loop {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    info!("Received Ctrl+C signal");
                    break;
                }
                Ok((stream, peer)) = listener.accept() => {
                    debug!("New connection {peer}");
                    let tls = tls.clone();

                    let ptr = self.clone();
                    let service = service_fn(move |request: hyper::Request<Incoming>| {
                        let path = request.uri().path().to_string();
                        let method = request.method().clone();
                        let service = ptr._services.get(&path).cloned();

                        let ptr = ptr.clone();
                        async move {
                            let response = if let Some(service) = service {
                                service.serve(ptr, peer, request).await
                            } else {
                                ResponseBuilder::default(StatusCode::NOT_FOUND)
                            };

                            debug!("[{} {}] {}", method, path, response.status());
                            Ok::<hyper::Response<BoxBody<Bytes, hyper::Error>>, hyper::Error>(response)
                        }
                    });

                    // Spawn a tokio task to serve multiple connections concurrently
                    task::spawn(async move {
                        let tls_stream = match tls.accept(stream).await {
                            Ok(s) => s,
                            Err(e) => {
                                error!("TLS accept error: {e}");
                                return;
                            }
                        };

                        if let Err(err) = Builder::new(TokioExecutor::new())
                            .serve_connection(TokioIo::new(tls_stream), service)
                            .await
                        {
                            error!("Error serving connection: {err:?} {err}");
                        }
                    });
                }
            }
        }

        Ok(())
    }
}
