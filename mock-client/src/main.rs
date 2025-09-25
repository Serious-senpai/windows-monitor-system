use std::error::Error;
use std::io::{Write, stdout};
use std::sync::Arc;
use std::time::Duration;

use async_compression::tokio::bufread::ZstdEncoder;
use clap::Parser;
use mock_client::cli::Arguments;
use mock_client::generator::EventGenerator;
use reqwest::{Certificate, Client, Identity, Url};
use tokio::io::AsyncReadExt;
use tokio::signal;
use tokio::sync::Semaphore;
use tokio::sync::mpsc::channel;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();

    print!("Password (hidden)>");
    let _ = stdout().flush();

    let password = rpassword::read_password().expect("Unable to read password");

    let generator = Arc::new(EventGenerator::new(arguments.pool_size));
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

    let (sender, mut receiver) = channel(2 * arguments.concurrency);
    let semaphore = Arc::new(Semaphore::new(arguments.concurrency));

    let pop = tokio::spawn(async move {
        while let Some(task) = receiver.recv().await {
            let _ = task.await;
        }
    });

    let url = Arc::new(arguments.url.clone());
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
    Ok(())
}
