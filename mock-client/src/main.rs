use std::error::Error;

use clap::Parser;
use mock_client::cli::Arguments;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();
    Ok(())
}
