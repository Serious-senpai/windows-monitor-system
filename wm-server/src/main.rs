use std::env;
use std::error::Error;
use std::fs::File;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use config_file::FromConfigFile;
use heed::byteorder::LittleEndian;
use heed::types::{U32, Unit};
use heed::{Database, EnvOpenOptions};
use log::{debug, error, info};
use reqwest::multipart::{Form, Part};
use tokio::fs;
use wm_common::logger::initialize_logger;
use wm_server::app::App;
use wm_server::cli::{Arguments, ServerAction};
use wm_server::configuration::Configuration;
use wm_server::rules;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();

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

    let app = Arc::new(App::async_new(configuration).await?);
    match arguments.command {
        ServerAction::Start => {
            app.run().await?;
        }
        ServerAction::UpdateRules => {
            let elastic = app
                .elastic()
                .await
                .expect("Unable to initialize Elasticsearch client");
            let kibana = elastic.kibana();

            let rules = rules::fetch_remote_rules().await?;
            let mut buf = vec![];
            for rule in rules {
                serde_json::to_writer(&mut buf, &rule)?;
                buf.push(b'\n');
            }

            let form = Form::new().part("file", Part::stream(buf).file_name("rules.ndjson"));
            match kibana
                .post("/api/detection_engine/rules/_import?overwrite=true")
                .header("kbn-xsrf", "true")
                .multipart(form)
                .send()
                .await
            {
                Ok(response) => {
                    info!("{}", response.status());

                    let text = response.text().await?;
                    info!("{text}");
                }
                Err(e) => {
                    error!("Unable to send request to Kibana: {e}");
                }
            }
        }
        ServerAction::FetchBlacklist { dest } => {
            if dest.exists() {
                error!("{} already exists, please remove it first", dest.display());
                return Ok(());
            }

            fs::create_dir_all(&dest).await?;
            let env = unsafe {
                EnvOpenOptions::new()
                    .map_size(10 << 20)
                    .open(&dest)
                    .unwrap()
            };

            let mut transaction = env.write_txn().unwrap();
            let db: Database<U32<LittleEndian>, Unit> =
                env.create_database(&mut transaction, None).unwrap();

            let client = reqwest::Client::new();
            let response = client
                .get("https://raw.githubusercontent.com/stamparm/ipsum/master/ipsum.txt")
                .send()
                .await
                .unwrap();

            let mut count = 0;
            for line in response.text().await.unwrap().lines() {
                if !line.starts_with('#') {
                    let ip = line
                        .split_ascii_whitespace()
                        .next()
                        .unwrap()
                        .parse::<Ipv4Addr>()
                        .unwrap();
                    let ip_u32 = ip.to_bits().to_le();
                    db.put(&mut transaction, &ip_u32, &())
                        .expect(&format!("Failed to insert IP {ip} (inserted {count})"));

                    count += 1;
                }
            }

            info!("Inserted {count} IPs into the blacklist database");
            transaction.commit()?;
        }
    }

    Ok(())
}
