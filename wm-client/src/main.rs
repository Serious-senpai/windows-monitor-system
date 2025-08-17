use std::env;
use std::error::Error;
use std::fs::File;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use config_file::FromConfigFile;
use log::{debug, info};
use tokio::task::JoinHandle;
use tokio::{fs, signal, task};
use wm_client::agent::Agent;
use wm_client::cli::{Arguments, ServiceAction};
use wm_client::configuration::Configuration;
use wm_common::error::RuntimeError;
use wm_common::logger::initialize_logger;
use wm_common::service::SC_MANAGER_ALL_ACCESS;
use wm_common::service::service_manager::ServiceManager;
use wm_common::service::status::ServiceState;

const SERVICE_NAME: &str = "Windows Monitor Agent Service\0";

async fn _runner(
    configuration: Configuration,
    handle: Option<JoinHandle<Result<(), &'static str>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let agent = Arc::new(Agent::new(configuration));

    let ptr = agent.clone();
    let agent_handle = tokio::spawn(async move {
        ptr.run().await;
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C signal");
        },
        _ = agent_handle => (),
    };

    debug!("Stopping agent");
    agent.stop().await;

    if let Some(h) = handle {
        h.await??;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();

    // TODO: Protect these paths
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

    match arguments.action {
        ServiceAction::Create => {
            debug!("Creating new service {SERVICE_NAME}");

            let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
            scm.create_service(
                SERVICE_NAME,
                &format!("{} start\0", executable_path.to_string_lossy()),
            )?;
        }
        ServiceAction::Start => {
            debug!("Checking service {SERVICE_NAME}");

            let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
            let status = scm.query_service_status(SERVICE_NAME)?;
            if status.current_state != ServiceState::Stopped {
                Err(RuntimeError::new("Service must be stopped before starting"))?;
            }

            debug!("Starting service {SERVICE_NAME}");

            let handle = task::spawn_blocking(|| {
                windows_services::Service::new().run(|_, command| {
                    info!("Service command: {command:?}");
                })
            });

            _runner(configuration, Some(handle)).await?;
        }
        ServiceAction::Delete => {
            debug!("Deleting service {SERVICE_NAME}");

            let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
            scm.delete_service(SERVICE_NAME)?;
        }
        ServiceAction::Process => {
            debug!("Running as a standalone process");

            _runner(configuration, None).await?;
        }
    };

    Ok(())
}
