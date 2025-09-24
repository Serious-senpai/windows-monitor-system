use std::error::Error;
use std::io::{Write, stdout};
use std::time::Duration;

use clap::Parser;
use mock_client::cli::Arguments;
use reqwest::{Certificate, Client, Identity};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();

    print!("Password (hidden)>");
    let _ = stdout().flush();

    let password = rpassword::read_password().expect("Unable to read password");

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

    todo!();
}
