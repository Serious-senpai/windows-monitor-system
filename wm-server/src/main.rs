use std::env;
use std::error::Error;
use std::fs::File;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use config_file::FromConfigFile;
use log::debug;
use tokio::fs;
use wm_common::logger::initialize_logger;
use wm_server::app::App;
use wm_server::configuration::Configuration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let executable_path = env::current_exe().expect("Failed to get current executable path");
    let app_directory = executable_path
        .parent()
        .expect("Failed to get application directory")
        .to_path_buf();

    let configuration = Arc::new(
        Configuration::from_config_file(app_directory.join("server-config.yml"))
            .expect("Failed to load configuration"),
    );

    let log_directory = app_directory.join("logs");
    fs::create_dir_all(&log_directory)
        .await
        .expect("Failed to create log directory");

    initialize_logger(
        configuration.log_level,
        File::create(log_directory.join(format!(
                "wm-server-{}.log",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards").as_millis()
            )))?,
    )?;
    debug!("Initialized logger");

    let app = Arc::new(App::new(configuration).await?);
    app.run().await?;

    Ok(())
}
