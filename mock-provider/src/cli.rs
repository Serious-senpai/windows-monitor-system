use clap::{Parser, crate_description, crate_version};

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    /// Number of temporary files to create and delete in each batch
    pub files_count: usize,

    /// Interval in milliseconds between each batch of file operations
    #[arg(long, default_value_t = 1000)]
    pub interval_ms: u64,
}
