use crate::error::{Error, ErrorKind, Result};

use std::fmt;
use std::io;

use fern::colors::Color;
use fern::FormatCallback;

fn log_format(out: FormatCallback, message: &fmt::Arguments, record: &log::Record) {
    let log_header = match (record.level(), record.target()) {
        (log::Level::Info, "ok") => "ok: ".to_owned(),
        (log::Level::Info, "changed") => "changed: ".to_owned(),
        (log::Level::Info, "skipping") => "skipping".to_owned(),
        (log::Level::Warn, _) => "[WARNING] ".to_owned(),
        (log::Level::Error, "task") => "failed: ".to_owned(),
        (log::Level::Error, _) => "[ERROR] ".to_owned(),
        (log::Level::Info, "task") => "TASK ".to_owned(),
        (log::Level::Info, _) => "".to_owned(),
        (log::Level::Debug, _) => "".to_owned(),
        (log::Level::Trace, s) => s.to_owned() + " - ",
    };
    out.finish(format_args!(
        "{color_line}{log_header}{message}{separator}\x1B[0m",
        color_line = format_args!(
            "\x1B[{}m",
            match (record.level(), record.target()) {
                (log::Level::Trace, "error") => Color::Red,
                (log::Level::Trace, _) => Color::BrightBlack,
                (log::Level::Debug, _) => Color::BrightBlue,
                (log::Level::Info, "changed") => Color::Yellow,
                (log::Level::Info, "ok") => Color::Green,
                (log::Level::Info, "skipping") => Color::Blue,
                (log::Level::Info, _) => Color::White,
                (log::Level::Warn, _) => Color::Magenta,
                (log::Level::Error, _) => Color::Red,
            }
            .to_fg_str()
        ),
        log_header = log_header.clone(),
        message = message.clone(),
        separator = match (record.level(), record.target()) {
            (log::Level::Info, "task") => vec![
                "*";
                term_size::dimensions().map(|(w, _)| w).unwrap_or(80)
                    - log_header.len()
                    - message.to_string().len()
            ]
            .join(""),
            (_, _) => "".to_string(),
        },
    ))
}

/// Setup logging in function of verbosity.
pub fn setup_logging(verbosity: u8) -> Result<()> {
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
