pub mod agent;
pub mod backup;
pub mod cli;
pub mod configuration;
pub mod http;
pub mod module;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
