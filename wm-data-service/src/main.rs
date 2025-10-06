use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::fs::File;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use config_file::FromConfigFile;
use fancy_regex::Regex;
use log::{debug, error, info};
use reqwest::multipart::{Form, Part};
use tokio::fs;
use wm_common::logger::initialize_logger;
use wm_data_service::app::App;
use wm_data_service::cli::{Arguments, ServiceAction};
use wm_data_service::configuration::Configuration;
use wm_data_service::rules;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();

    let executable_path = env::current_exe().expect("Failed to get current executable path");
    let app_directory = executable_path
        .parent()
        .expect("Failed to get application directory")
        .to_path_buf();

    let configuration = Arc::new(
        Configuration::from_config_file(app_directory.join("data-service-config.yml"))
            .expect("Failed to load configuration"),
    );

    let log_directory = app_directory.join("logs");
    fs::create_dir_all(&log_directory)
        .await
        .expect("Failed to create log directory");

    initialize_logger(
        configuration.log_level,
        File::create(log_directory.join(format!(
                "wm-data-service-{}.log",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards").as_millis()
            )))?,
    )?;
    debug!("Initialized logger");

    let app = App::new(configuration.clone()).expect("Failed to initialize application");
    match arguments.command {
        ServiceAction::Start => {
            app.run().await?;
        }
        ServiceAction::UpdateRules => {
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
        ServiceAction::RequiredFields => {
            let mut fields = HashSet::new();
            let pattern = Regex::new(
                r"(?<![\.\w])(?:@timestamp|agent|client|cloud|container|data_stream|destination|device|dll|dns|ecs|email|error|event|faas|file|gen_ai|group|host|http|labels|log|message|network|observer|orchestrator|organization|package|process|registry|related|rule|server|service|source|span|tags|threat|tls|trace|transaction|url|user|user_agent|volume|vulnerability)(?:\.[a-z_]+)+",
            )?;

            let rules = rules::fetch_remote_rules().await?;
            for rule in &rules {
                let query = rule["query"].as_str().unwrap_or_default();
                for capture in pattern.find_iter(query) {
                    fields.insert(capture?.as_str());
                }
            }

            let mut fields = fields.into_iter().collect::<Vec<&str>>();
            fields.sort();

            info!("Required ECS fields ({}):", fields.len());
            for field in fields {
                info!("{field}");
            }
        }
    }

    Ok(())
}
