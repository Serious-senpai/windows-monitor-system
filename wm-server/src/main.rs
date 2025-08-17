use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, io};

use config_file::FromConfigFile;
use elasticsearch::http::response::Response;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use log::{debug, error, info, warn};
use rustls::ServerConfig;
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::{fs, signal, task};
use tokio_rustls::TlsAcceptor;
use wm_common::logger::initialize_logger;
use wm_server::configuration::Configuration;
use wm_server::routes::abc::Service;
use wm_server::routes::login::LoginService;
use wm_server::routes::trace::TraceService;
use wm_server::utils;

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

// Load public certificate from file.
fn _load_certs(filename: &PathBuf) -> io::Result<Vec<CertificateDer<'static>>> {
    // Open certificate file.
    let certfile = File::open(filename)?;
    let mut reader = io::BufReader::new(certfile);

    // Load and return certificate.
    rustls_pemfile::certs(&mut reader).collect()
}

// Load private key from file.
fn _load_private_key(filename: &PathBuf) -> io::Result<PrivateKeyDer<'static>> {
    // Open keyfile.
    let keyfile = File::open(filename)?;
    let mut reader = io::BufReader::new(keyfile);

    // Load and return a single private key.
    rustls_pemfile::private_key(&mut reader).map(|key| key.unwrap())
}

static TABLE: LazyLock<RwLock<HashMap<String, Arc<dyn Service>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let executable_path = env::current_exe().expect("Failed to get current executable path");
    let app_directory = executable_path
        .parent()
        .expect("Failed to get application directory")
        .to_path_buf();

    let configuration = Configuration::from_config_file(app_directory.join("config.yml"))
        .expect("Failed to load configuration");

    let log_directory = app_directory.join("logs");
    fs::create_dir_all(&log_directory)
        .await
        .expect("Failed to create log directory");

    initialize_logger(
        configuration.log_level,
        File::create(log_directory.join(format!(
                "wm-{}.log",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards").as_millis()
            )))?,
    )?;
    debug!("Initialized logger");

    let services: Vec<Arc<dyn Service>> =
        vec![Arc::new(LoginService {}), Arc::new(TraceService {})];

    {
        let mut table = TABLE.write().await;
        for service in services {
            table.insert(service.route().to_string(), service);
        }
    }

    let _ = aws_lc_rs::default_provider().install_default();

    let addr = SocketAddr::from(([0, 0, 0, 0], configuration.port));
    let certs = _load_certs(&configuration.certificate)?;
    let key = _load_private_key(&configuration.private_key)?;

    let listener = TcpListener::bind(addr).await?;
    let mut cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| io::Error::other(e.to_string()))?;
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];

    let tls = TlsAcceptor::from(Arc::new(cfg));
    let service = service_fn(async |request: hyper::Request<Incoming>| {
        let table = TABLE.read().await;
        let path = request.uri().path().to_string();

        let method = request.method().clone();
        let response = if let Some(service) = table.get(&path) {
            service.serve(request).await
        } else {
            utils::not_found()
        };

        debug!("[{} {}] {}", method, path, response.status());
        Ok::<hyper::Response<BoxBody<Bytes, hyper::Error>>, hyper::Error>(response)
    });

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received Ctrl+C");
                break;
            }
            Ok((stream, _)) = listener.accept() => {
                debug!("New connection {}", stream.peer_addr()?);
                let tls = tls.clone();

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
