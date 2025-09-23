use clap::{Parser, Subcommand, crate_description, crate_version};

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: ServerAction,
}

#[derive(Debug, Subcommand)]
#[clap(rename_all = "kebab_case")]
pub enum ServerAction {
    /// Start the Windows service or run in console mode if not running as a service
    Start,

    /// Update Elasticsearch detection rules from the remote repository
    UpdateRules,

    /// List ECS fields required by Elasticsearch detection rules
    RequiredFields,
}
