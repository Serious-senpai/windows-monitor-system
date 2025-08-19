use std::env;
use std::error::Error;
use std::fs::File;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use config_file::FromConfigFile;
use log::{debug, error, info};
use tokio::fs;
use windows::Win32::System::Services::SC_MANAGER_ALL_ACCESS;
use wm_client::cli::{Arguments, ServiceAction};
use wm_client::configuration::Configuration;
use wm_client::runner::AgentRunner;
use wm_common::error::RuntimeError;
use wm_common::logger::initialize_logger;
use wm_common::service::service_manager::ServiceManager;
use wm_common::service::status::ServiceState;

const SERVICE_NAME: &str = "Windows Monitor Agent Service";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();

    // TODO: Protect these paths
    let executable_path = env::current_exe().expect("Failed to get current executable path");
    let app_directory = executable_path
        .parent()
        .expect("Failed to get application directory")
        .to_path_buf();

    let configuration = Arc::new(
        Configuration::from_config_file(app_directory.join("client-config.yml"))
            .expect("Failed to load configuration"),
    );

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
                &format!("{SERVICE_NAME}\0"),
                &format!("{} start\0", executable_path.to_string_lossy()),
            )?;

            info!("Done");
        }
        ServiceAction::Start => {
            if windows_service_detector::is_running_as_windows_service() == Ok(true) {
                debug!("Checking service {SERVICE_NAME}");

                let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
                let status = scm.query_service_status(&format!("{SERVICE_NAME}\0"))?;
                if status.current_state != ServiceState::StartPending {
                    Err(RuntimeError::new(format!(
                        "Invalid state {:?}",
                        status.current_state
                    )))?;
                }

                debug!("Starting service {SERVICE_NAME}");
                let mut runner = AgentRunner::new::<true>(configuration.clone());
                runner.run().await?;
            } else {
                error!("This command can only be run as a Windows service");
            }
        }
        ServiceAction::Delete => {
            debug!("Deleting service {SERVICE_NAME}");

            let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
            scm.delete_service(&format!("{SERVICE_NAME}\0"))?;

            info!("Done");
        }
        ServiceAction::Process => {
            debug!("Running as a standalone process");

            let mut runner = AgentRunner::new::<false>(configuration.clone());
            runner.run().await?;
        }
    };

    Ok(())
}
