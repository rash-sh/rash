use crate::error::{Error, ErrorKind, Result};
use crate::utils::get_terminal_width;

use std::fmt;
use std::io;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use clap::ValueEnum;
use console::{Style, style};
use fern::FormatCallback;
use similar::{Change, ChangeTag, TextDiff};

struct Line(Option<usize>);

const COLOR_BRIGHT_BLUE: u8 = 33;
const COLOR_BRIGHT_BLACK: u8 = 244;

#[derive(Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum Output {
    /// ansible style output with tasks and changed outputs
    Ansible,
    /// print module outputs without any extra details, omitting task names and separators.
    Raw,
    /// output results as JSON for machine parsing
    Json,
}

static OUTPUT_FORMAT: AtomicI32 = AtomicI32::new(0);
static LOG_SUPPRESSION_DEPTH: AtomicUsize = AtomicUsize::new(0);

fn output_to_int(output: &Output) -> i32 {
    match output {
        Output::Ansible => 0,
        Output::Raw => 1,
        Output::Json => 2,
    }
}

fn int_to_output(val: i32) -> Output {
    match val {
        1 => Output::Raw,
        2 => Output::Json,
        _ => Output::Ansible,
    }
}

pub fn set_output_format(output: &Output) {
    OUTPUT_FORMAT.store(output_to_int(output), Ordering::SeqCst);
}

pub fn get_output_format() -> Output {
    int_to_output(OUTPUT_FORMAT.load(Ordering::SeqCst))
}

pub fn is_json_output() -> bool {
    get_output_format() == Output::Json
}

pub fn logs_suppressed() -> bool {
    LOG_SUPPRESSION_DEPTH.load(Ordering::SeqCst) > 0
}

/// RAII guard used by `no_log` tasks. Nested secret tasks are safe because suppression is counted.
pub struct LogSuppressionGuard;

pub fn suppress_logs() -> LogSuppressionGuard {
    LOG_SUPPRESSION_DEPTH.fetch_add(1, Ordering::SeqCst);
    LogSuppressionGuard
}

impl Drop for LogSuppressionGuard {
    fn drop(&mut self) {
        LOG_SUPPRESSION_DEPTH.fetch_sub(1, Ordering::SeqCst);
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Some(idx) => write!(f, "{:<4}", idx + 1),
            None => write!(f, "    "),
        }
    }
}

fn print_diff<T>(iter: T, prefix: &str, style: &Style)
where
    T: IntoIterator,
    T::Item: fmt::Display,
{
    if !logs_suppressed() && log_enabled!(target: "diff", log::Level::Info) {
        iter.into_iter()
            .for_each(|x| println!("{}{}", style.apply_to(prefix), style.apply_to(x)));
    };
}

pub fn add<T>(iter: T)
where
    T: IntoIterator,
    T::Item: fmt::Display,
{
    print_diff(iter, "+ ", &Style::new().green());
}

pub fn remove<T>(iter: T)
where
    T: IntoIterator,
    T::Item: fmt::Display,
{
    print_diff(iter, "- ", &Style::new().red());
}

pub fn diff_files<T, U>(original: T, modified: U)
where
    T: std::string::ToString,
    U: std::string::ToString,
{
    if logs_suppressed() || !log_enabled!(target: "diff", log::Level::Info) {
        return;
    }
    let o = original.to_string();
    let m = modified.to_string();
    let text_diff = TextDiff::from_lines(&o, &m);

    for (idx, group) in text_diff.grouped_ops(3).iter().enumerate() {
        if idx > 0 {
            println!("{:-^1$}", "-", get_terminal_width());
        }
        for op in group {
            for change in text_diff.iter_inline_changes(op) {
                let (sign, s) = match change.tag() {
                    ChangeTag::Delete => ("-", Style::new().red()),
                    ChangeTag::Insert => ("+", Style::new().green()),
                    ChangeTag::Equal => (" ", Style::new().dim()),
                };
                print!(
                    "{}{} |{}",
                    style(Line(change.old_index())).dim(),
                    style(Line(change.new_index())).dim(),
                    s.apply_to(sign).bold(),
                );
                for (emphasized, value) in change.iter_strings_lossy() {
                    if emphasized {
                        print!("{}", s.apply_to(value).underlined().on_black());
                    } else {
                        print!("{}", s.apply_to(value));
                    }
                }
                if change.missing_newline() {
                    println!();
                }
            }
        }
    }
}

fn format_change<T: similar::DiffableStr + ?Sized>(change: Change<&T>) -> String {
    match change.tag() {
        ChangeTag::Equal => format!("  {change}"),
        ChangeTag::Delete => Style::new()
            .red()
            .apply_to(format!("- {change}"))
            .to_string(),
        ChangeTag::Insert => Style::new()
            .green()
            .apply_to(format!("+ {change}"))
            .to_string(),
    }
}

pub fn diff<T, U>(original: T, modified: U)
where
    T: std::string::ToString,
    U: std::string::ToString,
{
    if logs_suppressed() || !log_enabled!(target: "diff", log::Level::Info) {
        return;
    }
    let o = original.to_string();
    let m = modified.to_string();
    let text_diff = TextDiff::from_lines(&o, &m);
    let diff_str = text_diff
        .iter_all_changes()
        .map(format_change)
        .collect::<Vec<String>>()
        .join("");
    print!("{diff_str}");
}

fn ansible_log_format(out: FormatCallback, message: &fmt::Arguments, record: &log::Record) {
    let level = record.level();
    let target = record.target();
    let log_header = match (level, target) {
        (log::Level::Error, "task") => "failed: ".to_owned(),
        (log::Level::Error, _) => "[ERROR] ".to_owned(),
        (log::Level::Warn, _) => "[WARNING] ".to_owned(),
        (log::Level::Info, "changed") => "changed: ".to_owned(),
        (log::Level::Info, "changed_empty") => "changed".to_owned(),
        (log::Level::Info, "ignoring") => "[ignoring error] ".to_owned(),
        (log::Level::Info, "ok") => "ok: ".to_owned(),
        (log::Level::Info, "ok_empty") => "ok".to_owned(),
        (log::Level::Info, "task") => "TASK ".to_owned(),
        (log::Level::Info, _) => "".to_owned(),
        (log::Level::Debug, _) => "".to_owned(),
        (log::Level::Trace, s) => s.to_owned() + " - ",
    };

    let style = match (level, target) {
        (log::Level::Error, _) => Style::new().red(),
        (log::Level::Warn, _) => Style::new().magenta(),
        (log::Level::Info, "changed" | "changed_empty") => Style::new().yellow(),
        (log::Level::Info, "diff") => Style::new().color256(COLOR_BRIGHT_BLACK),
        (log::Level::Info, "ignoring") => Style::new().blue(),
        (log::Level::Info, "ok" | "ok_empty") => Style::new().green(),
        (log::Level::Info, _) => Style::new().white(),
        (log::Level::Debug, _) => Style::new().color256(COLOR_BRIGHT_BLUE),
        (log::Level::Trace, _) => Style::new().color256(COLOR_BRIGHT_BLACK),
    };
    let line = format!(
        "{log_header}{message}{separator}",
        separator = match (level, target) {
            (log::Level::Info, "task") => vec![
                "*";
                {
                    let term_width = get_terminal_width();
                    let message_total_len = log_header.len() + message.to_string().len();
                    if term_width > message_total_len {
                        term_width - message_total_len
                    } else {
                        (message_total_len / term_width + 1) * term_width - message_total_len
                    }
                }
            ]
            .join(""),
            _ => "".to_owned(),
        },
    );
    out.finish(format_args!("{}", style.apply_to(line)))
}

fn raw_log_format(out: FormatCallback, message: &fmt::Arguments, _record: &log::Record) {
    out.finish(format_args!("{message}"))
}

pub fn setup_logging(verbosity: u8, diff: &bool, output: &Output) -> Result<()> {
    set_output_format(output);

    let mut base_config = fern::Dispatch::new().filter(|_| !logs_suppressed());
    base_config = match verbosity {
        0 => base_config.level(log::LevelFilter::Info),
        1 => base_config.level(log::LevelFilter::Debug),
        _ => base_config.level(log::LevelFilter::Trace),
    };

    base_config = match diff {
        false => base_config.level_for("diff", log::LevelFilter::Error),
        true => base_config.level_for("diff", log::LevelFilter::Info),
    };

    let is_internal = std::env::var(crate::task::RASH_INTERNAL_TASK_FLAG).is_ok();
    base_config = match (output, is_internal) {
        (Output::Raw | Output::Json, _) | (_, true) => {
            base_config.level_for("task", log::LevelFilter::Error)
        }
        _ => base_config,
    };

    let log_format = match output {
        Output::Ansible => ansible_log_format,
        Output::Raw | Output::Json => raw_log_format,
    };

    base_config
        .format(log_format)
        .chain(
            fern::Dispatch::new()
                .filter(|metadata| metadata.level() >= log::LevelFilter::Warn)
                .chain(io::stdout()),
        )
        .chain(
            fern::Dispatch::new()
                .level(log::LevelFilter::Warn)
                .chain(io::stderr()),
        )
        .apply()
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppression_guard_is_nested() {
        assert!(!logs_suppressed());
        let outer = suppress_logs();
        assert!(logs_suppressed());
        {
            let _inner = suppress_logs();
            assert!(logs_suppressed());
        }
        assert!(logs_suppressed());
        drop(outer);
        assert!(!logs_suppressed());
    }
}
