use std::env;
use std::error::Error;
use std::fs::File as BlockingFile;
use std::io::{Write, stdout};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_compression::tokio::write::ZstdDecoder;
use clap::Parser;
use config_file::FromConfigFile;
use log::{debug, info};
use tokio::{fs, io, task};
use windows::Win32::System::Services::SC_MANAGER_ALL_ACCESS;
use wm_client::cli::{Arguments, ServiceAction};
use wm_client::configuration::Configuration;
use wm_client::runner::AgentRunner;
use wm_common::credential::CredentialManager;
use wm_common::error::RuntimeError;
use wm_common::logger::initialize_logger;
use wm_common::service::service_manager::ServiceManager;
use wm_common::service::status::ServiceState;

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
        BlockingFile::create(log_directory.join(format!(
                "wm-client-{}.log",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards").as_millis()
            )))?,
    )?;
    debug!("Initialized logger");

    match arguments.command {
        ServiceAction::Create => {
            info!("Creating new service {}", configuration.service_name);

            let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
            scm.create_service(
                &format!("{}\0", configuration.service_name),
                &format!("{} start\0", executable_path.to_string_lossy()),
            )?;

            info!("Done");
        }
        ServiceAction::Start => {
            if windows_service_detector::is_running_as_windows_service() == Ok(true) {
                info!("Checking service {}", configuration.service_name);

                let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
                let status =
                    scm.query_service_status(&format!("{}\0", configuration.service_name))?;
                if status.current_state != ServiceState::StartPending {
                    Err(RuntimeError::new(format!(
                        "Invalid state {:?}",
                        status.current_state
                    )))?;
                }

                info!("Starting service {}", configuration.service_name);
                let mut runner = AgentRunner::new::<true>(configuration.clone());
                runner.run().await?;
            } else {
                info!("Running as a standalone process");

                let mut runner = AgentRunner::new::<false>(configuration.clone());
                runner.run().await?;
            }
        }
        ServiceAction::Delete => {
            info!("Deleting service {}", configuration.service_name);

            let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
            scm.delete_service(&format!("{}\0", configuration.service_name))?;

            info!("Done");
        }
        ServiceAction::Password => task::spawn_blocking(move || {
            let mut stdout = stdout();
            print!("Password (hidden)>");
            let _ = stdout.flush();

            let password = rpassword::read_password().expect("Unable to read password");
            CredentialManager::write(
                &mut format!("{}\0", configuration.windows_credential_manager_key),
                password.as_bytes(),
            )
            .expect("Failed to store password");

            info!("Password stored to Windows Credential Manager");
        })
        .await
        .expect("Unable to read password"),
        ServiceAction::Zstd { source, dest } => {
            let mut source_file = fs::File::open(&source).await?;
            let mut dest_file = fs::File::create_new(&dest).await?;

            let mut decompressor = ZstdDecoder::new(&mut dest_file);
            io::copy(&mut source_file, &mut decompressor)
                .await
                .expect("Failure during decompression");
        }
    };

    Ok(())
}
