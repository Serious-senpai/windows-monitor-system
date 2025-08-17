use std::io::Write;

use log::{LevelFilter, SetLoggerError};
use serde::{Deserialize, Serialize};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn to_level_filter(&self) -> LevelFilter {
        match self {
            Self::Off => LevelFilter::Off,
            Self::Error => LevelFilter::Error,
            Self::Warn => LevelFilter::Warn,
            Self::Info => LevelFilter::Info,
            Self::Debug => LevelFilter::Debug,
            Self::Trace => LevelFilter::Trace,
        }
    }
}

pub fn initialize_logger<W>(level: LogLevel, writer: W) -> Result<(), SetLoggerError>
where
    W: Write + Send + 'static,
{
    CombinedLogger::init(vec![
        WriteLogger::new(
            level.to_level_filter(),
            ConfigBuilder::new()
                .set_location_level(LevelFilter::Debug)
                .build(),
            writer,
        ),
        TermLogger::new(
            level.to_level_filter(),
            ConfigBuilder::new()
                .set_location_level(LevelFilter::Debug)
                .build(),
            TerminalMode::Stderr,
            ColorChoice::Auto,
        ),
    ])
}
