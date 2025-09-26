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
    /// Start consuming messages from RabbitMQ, processing and sending them to Elasticsearch
    Start,

    /// Update Elasticsearch detection rules from the remote repository
    UpdateRules,

    /// List ECS fields required by Elasticsearch detection rules
    RequiredFields,
}
