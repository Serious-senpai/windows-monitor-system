use clap::{Parser, crate_description, crate_version};
use reqwest::Url;

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    /// Base URL of the running server instance
    pub url: Url,

    /// Number of maximum concurrent requests
    pub concurrency: usize,

    /// Number of requests in the request pool to select from.
    ///
    /// In order to improve client performance, a pool of requests is pre-generated
    /// at the beginning and requests are randomly selected from this pool.
    pub pool_size: usize,
}
