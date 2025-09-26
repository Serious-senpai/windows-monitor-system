use clap::{Parser, Subcommand, crate_description, crate_version};

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: ServiceAction,
}

#[derive(Debug, Subcommand)]
#[clap(rename_all = "kebab_case")]
pub enum ServiceAction {
    /// Start the API service
    Start,
}
