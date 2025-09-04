pub mod agent;
pub mod backup;
pub mod cli;
pub mod configuration;
pub mod http;
pub mod module;
pub mod runner;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
