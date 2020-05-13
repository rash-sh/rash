use crate::error::{Error, ErrorKind, Result};

use std::fmt;
use std::io;

use fern::colors::Color;
use fern::FormatCallback;
use log;
use term_size;

fn log_format(out: FormatCallback, message: &fmt::Arguments, record: &log::Record) {
    out.finish(format_args!(
        "{color_line}{log_header}{message}{separator}\x1B[0m",
        color_line = format_args!(
            "\x1B[{}m",
            match (record.level(), record.target()) {
                (log::Level::Trace, _) => Color::BrightBlack,
                (log::Level::Debug, _) => Color::BrightBlue,
                (log::Level::Info, "changed") => Color::Yellow,
                (log::Level::Info, "ok") => Color::Green,
                (log::Level::Info, _) => Color::White,
                (log::Level::Warn, _) => Color::Magenta,
                (log::Level::Error, _) => Color::Red,
            }
            .to_fg_str()
        ),
        log_header = match (record.level(), record.target()) {
            (log::Level::Info, "ok") => "ok: ",
            (log::Level::Info, "changed") => "changed: ",
            (log::Level::Warn, _) => "[WARNING] ",
            (log::Level::Error, "task") => "failed: ",
            (log::Level::Error, _) => "[ERROR] ",
            (log::Level::Info, "task") => "TASK ",
            (log::Level::Info, _) => "",
            (log::Level::Debug, _) => "",
            (log::Level::Trace, s) => s,
        }
        .clone(),
        message = message,
        separator = match (record.level(), record.target()) {
            (log::Level::Info, "task") =>
                vec!["*"; term_size::dimensions().map(|(w, _)| w).unwrap_or(80)].join(""),
            (_, _) => "".to_string(),
        },
    ))
}

pub fn setup_logging(verbosity: u64) -> Result<()> {
    let mut base_config = fern::Dispatch::new();

    base_config = match verbosity {
        0 => base_config.level(log::LevelFilter::Info),
        1 => base_config.level(log::LevelFilter::Debug),
        _ => base_config.level(log::LevelFilter::Trace),
    };

    let stdout_config = fern::Dispatch::new().format(log_format).chain(io::stdout());

    base_config
        .chain(stdout_config)
        .apply()
        .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?;

    Ok(())
}
