use clap::{Parser, ValueEnum, crate_description, crate_version};

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    #[arg(value_enum)]
    pub action: ServiceAction,
}

#[derive(Debug, Clone, ValueEnum)]
#[clap(rename_all = "kebab_case")]
pub enum ServiceAction {
    Create,
    Start,
    Delete,
}
