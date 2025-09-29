use clap::{Parser, Subcommand, crate_description, crate_version};
use reqwest::Url;

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    #[command(subcommand)]
    pub action: Utility,
}

#[derive(Debug, Subcommand)]
#[clap(rename_all = "kebab_case")]
pub enum Utility {
    /// Start the mocking client
    MockClient {
        /// Base URL of the running server instance
        url: Url,

        /// Number of maximum concurrent requests
        #[arg(long, default_value_t = 5)]
        concurrency: usize,

        /// Number of requests in the request pool to select from.
        ///
        /// In order to improve client performance, a pool of requests is pre-generated
        /// at the beginning and requests are randomly selected from this pool.
        #[arg(long, default_value_t = 100)]
        pool_size: usize,
    },

    /// Start the mocking event generator
    MockEvents {
        /// Number of temporary files to create and delete in each batch
        files_count: usize,

        /// Interval in milliseconds between each batch of file operations
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
    },

    /// Update the password in Registry with the compile-time value
    UseDefaultPassword {
        /// The name of the Registry entry to update
        key_name: String,
    },
}
