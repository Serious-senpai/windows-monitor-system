use std::env;
use std::error::Error;
use std::fs::File as BlockingFile;
use std::io::{Write, stdout};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_compression::tokio::write::ZstdDecoder;
use clap::Parser;
use config_file::FromConfigFile;
use log::{debug, info, warn};
use mimalloc::MiMalloc;
use tokio::runtime::Builder;
use tokio::{fs, io, signal, task};
use windows::Win32::System::Services::SC_MANAGER_ALL_ACCESS;
use windows_services::{Command, Service};
use wm_client::agent::Agent;
use wm_client::cli::{Arguments, ServiceAction};
use wm_client::configuration::Configuration;
use wm_client::module::Module;
use wm_common::error::RuntimeError;
use wm_common::logger::initialize_logger;
use wm_common::registry::RegistryKey;
use wm_common::service::service_manager::ServiceManager;
use wm_common::service::status::ServiceState;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn _open_registry_password(config: &Configuration) -> RegistryKey {
    RegistryKey::new(&format!("{}\0", config.password_registry_key))
        .expect("Failed to open registry key")
}

fn _read_password(prompt: &str) -> String {
    let mut stdout = stdout();
    print!("{prompt}");
    let _ = stdout.flush();

    rpassword::read_password().expect("Unable to read password")
}

fn main() {
    let arguments = Arguments::parse();
    let executable_path = env::current_exe().expect("Failed to get current executable path");
    let app_directory = executable_path
        .parent()
        .expect("Failed to get application directory")
        .to_path_buf();
    let configuration = Configuration::from_config_file(app_directory.join("client-config.yml"))
        .expect("Failed to load configuration");

    let rt = Builder::new_multi_thread()
        .enable_all()
        .thread_name_fn(|| {
            static ID: AtomicUsize = AtomicUsize::new(0);
            let id = ID.fetch_add(1, Ordering::SeqCst);
            format!("tokio-runtime-worker-{id}")
        })
        .worker_threads(configuration.runtime_threads)
        .build()
        .expect("Failed to create Tokio runtime");

    rt.block_on(async_main(
        arguments,
        executable_path,
        app_directory,
        configuration,
    ))
    .expect("Runtime completed with error");
}

async fn async_main(
    arguments: Arguments,
    executable_path: PathBuf,
    app_directory: PathBuf,
    configuration: Configuration,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let configuration = Arc::new(configuration);

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
                &format!("{} start\0", executable_path.display()),
            )?;

            // let password = _read_password("Administrator password (hidden)>");
            // scm.change_service_user(
            //     &format!("{}\0", configuration.service_name),
            //     ".\\Administrator\0",
            //     &format!("{password}\0"),
            // )?;

            info!(
                "To start service, run: sc start \"{}\"",
                configuration.service_name
            );
            info!(
                "To query service, run: sc query \"{}\"",
                configuration.service_name
            );
            info!(
                "To stop service, run: sc stop \"{}\"",
                configuration.service_name
            );
        }
        ServiceAction::Start => {
            // let job = AssignJobGuard::new("wm-client-job-object")?;
            // job.cpu_limit(0.01)?;

            let key = _open_registry_password(&configuration);
            let value = key.read().expect("Failed to read registry value");
            let password = String::from_utf8(value).expect("Registry password is not valid UTF-8");

            let agent = Arc::new(Agent::async_new(configuration.clone(), &password).await);
            let s_handle = if windows_service_detector::is_running_as_windows_service() == Ok(true)
            {
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

                let agent = agent.clone();
                Some(task::spawn_blocking(move || {
                    Service::new().can_stop().run(|_, command| {
                        debug!("Received service command: {command:?}");

                        match command {
                            Command::Stop => {
                                info!("Stopping service");
                                agent.stop();
                            }
                            _ => {
                                warn!("Unsupported service command {command:?}")
                            }
                        }
                    })
                }))
            } else {
                info!("Running as a standalone process");
                None
            };

            let agent_cloned = agent.clone();
            let mut a_handle = tokio::spawn(agent_cloned.run());

            tokio::select! {
                _ = signal::ctrl_c() => {
                    info!("Received Ctrl+C signal");
                    agent.stop();
                },
                _ = &mut a_handle => {
                    info!("Agent task completed itself");
                },
            };

            if let Some(s_handle) = s_handle {
                s_handle.await??;
            }
            a_handle.await??;
        }
        ServiceAction::Delete => {
            info!("Deleting service {}", configuration.service_name);

            let scm = ServiceManager::new(SC_MANAGER_ALL_ACCESS)?;
            scm.delete_service(&format!("{}\0", configuration.service_name))?;

            info!("Done");
        }
        ServiceAction::Password => task::spawn_blocking(move || {
            let password = _read_password("Password (hidden)>");
            let key = _open_registry_password(&configuration);
            key.store(password.as_bytes())
                .expect("Failed to store registry value");
            key.allow_only(&["S-1-5-18\0", "S-1-5-32-544\0"])
                .expect("Failed to set registry permissions");

            info!("Password stored to Windows Credential Manager");
        })
        .await
        .expect("Unable to set password"),
        ServiceAction::Zstd { source, dest } => {
            let mut source_file = fs::File::open(&source).await?;
            let mut dest_file = fs::File::create_new(&dest).await?;

            let mut decompressor = ZstdDecoder::new(&mut dest_file);
            let bytes = io::copy(&mut source_file, &mut decompressor)
                .await
                .expect("Failure during decompression");

            info!(
                "Decompressed {bytes} bytes from {} to {}",
                source.display(),
                dest.display()
            );
        }
    };

    Ok(())
}
