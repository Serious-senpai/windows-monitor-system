use std::error::Error;
use std::io::{Write, stdin, stdout};
use std::sync::Arc;
use std::time::Duration;
use std::{env, process};

use async_compression::tokio::bufread::ZstdEncoder;
use chrono::Local;
use clap::Parser;
use reqwest::{Certificate, Client, Identity, Url};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc::channel;
use tokio::sync::{Semaphore, SetOnce};
use tokio::time::sleep;
use tokio::{fs, signal};
use utility::cli::{Arguments, Utility};
use utility::generator::EventGenerator;
use wm_common::registry::RegistryKey;

async fn request(
    client: Client,
    base_url: Arc<Url>,
    generator: Arc<EventGenerator>,
    semaphore: Arc<Semaphore>,
) {
    let mut input = Vec::with_capacity(150 * 1024);
    while input.len() < 100 * 1024 {
        let event = generator.get_event();
        input.extend_from_slice(event);
        input.push(b'\n');
    }

    let mut encoder = ZstdEncoder::new(input.as_slice());

    let mut buffer = Vec::with_capacity(5 * 1024);
    encoder
        .read_to_end(&mut buffer)
        .await
        .expect("Failed to compress data");

    #[allow(clippy::redundant_pattern_matching)] // required to acquire semaphore
    if let Ok(_) = semaphore.acquire().await {
        match client
            .post(base_url.join("/trace").expect("Unable to build URL"))
            .body(buffer)
            .send()
            .await
        {
            Ok(response) => {
                if !response.status().is_success() {
                    println!("{}", response.status());
                }
            }
            Err(e) => println!("Failed to send trace event to server: {e}"),
        }
    }
}

async fn mock_client(pool_size: usize, concurrency: usize, url: Url) {
    print!("Password (hidden)>");
    let _ = stdout().flush();
    let password = rpassword::read_password().expect("Unable to read password");

    let generator = Arc::new(EventGenerator::new(pool_size));
    let client = Client::builder()
        .add_root_certificate(
            Certificate::from_pem(include_bytes!("../../cert/server.pem"))
                .expect("Failed to load server certificate"),
        )
        .identity(
            Identity::from_pkcs12_der(
                include_bytes!(concat!(env!("OUT_DIR"), "/client.pfx")),
                &password,
            )
            .expect("Failed to load client identity"),
        )
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("Failed to create HTTP client");

    let (sender, mut receiver) = channel(2 * concurrency);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let pop = tokio::spawn(async move {
        while let Some(task) = receiver.recv().await {
            let _ = task.await;
        }
    });

    let url = Arc::new(url);
    let push = tokio::spawn(async move {
        loop {
            let task = tokio::spawn(request(
                client.clone(),
                url.clone(),
                generator.clone(),
                semaphore.clone(),
            ));

            tokio::select! {
                biased;
                _ = signal::ctrl_c() => {
                    println!("Received Ctrl-C");
                    break;
                },
                _ = sender.send(task) => {},
            }
        }
    });

    let _ = tokio::join!(pop, push);
}

async fn mock_events(files_count: usize, interval_ms: u64) {
    let executable_path = env::current_exe().expect("Failed to get current executable path");
    let app_directory = executable_path
        .parent()
        .expect("Failed to get application directory")
        .to_path_buf();

    print!("Current PID is {}. Press Enter to start.", process::id());
    let _ = stdout().flush();

    let mut buf = String::new();
    let _ = stdin().read_line(&mut buf);

    let stopped = Arc::new(SetOnce::new());
    let stopped_clone = stopped.clone();
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;

        println!("Received Ctrl+C");
        let _ = stopped_clone.set(());
    });

    while stopped.get().is_none() {
        let mut tasks = vec![];
        for index in 0..files_count {
            let path = app_directory.join(format!("mock-{index}.tmp"));
            tasks.push(tokio::spawn(async move {
                let file = fs::File::create(&path)
                    .await
                    .unwrap_or_else(|_| panic!("Failed to create {}", path.display()));
                drop(file);
                fs::remove_file(&path)
                    .await
                    .unwrap_or_else(|_| panic!("Failed to remove {}", path.display()));
            }));
        }

        for task in tasks {
            if let Err(e) = task.await {
                println!("Task failed with error: {e}");
            }
        }

        println!("{} Finished 1 batch", Local::now());
        sleep(Duration::from_millis(interval_ms)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();

    match arguments.action {
        Utility::MockClient {
            url,
            concurrency,
            pool_size,
        } => mock_client(pool_size, concurrency, url).await,
        Utility::MockEvents {
            files_count,
            interval_ms,
        } => mock_events(files_count, interval_ms).await,
        Utility::UseDefaultPassword { key_name } => {
            let key =
                RegistryKey::new(&format!("{key_name}\0")).expect("Failed to open registry key");
            key.store(env!("WINDOWS_MONITOR_PASSWORD").as_bytes())
                .expect("Failed to store registry value");
        }
    }

    Ok(())
}
