use std::path::PathBuf;

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
    /// Create the Windows service
    Create,

    /// Start the Windows service or run in console mode if not running as a service
    Start,

    /// Delete the Windows service
    Delete,

    /// Update the password stored in Windows Credential Manager
    Password,

    /// Extract a zstd-compressed binary file
    Zstd {
        /// Path to the file containing zstd-compressed binary data
        source: PathBuf,

        /// Path to write the extracted binary data to
        dest: PathBuf,
    },
}
