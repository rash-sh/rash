use crate::error::{Error, ErrorKind, Result};

use std::fmt;
use std::io;

use clap::ValueEnum;
use console::{style, Style};
use fern::FormatCallback;
use similar::{Change, ChangeTag, TextDiff};

struct Line(Option<usize>);

const COLOR_BRIGHT_BLUE: u8 = 33;
const COLOR_BRIGHT_BLACK: u8 = 244;

#[derive(Clone, Debug, ValueEnum)]
pub enum Output {
    /// ansible style output with tasks and changed outputs
    Ansible,
    /// print module outputs without any extra details, omitting task names and separators.
    Raw,
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            None => write!(f, "    "),
            Some(idx) => write!(f, "{:<4}", idx + 1),
        }
    }
}

fn get_terminal_width() -> usize {
    term_size::dimensions().map(|(w, _)| w).unwrap_or(80)
}

/// Print iterator.
fn print_diff<T>(iter: T, prefix: &str, style: &Style)
where
    T: IntoIterator,
    T::Item: fmt::Display,
{
    if log_enabled!(target: "diff", log::Level::Info) {
        iter.into_iter()
            .for_each(|x| println!("{}{}", style.apply_to(prefix), style.apply_to(x)));
    };
}

/// Print add iterator.
pub fn add<T>(iter: T)
where
    T: IntoIterator,
    T::Item: fmt::Display,
{
    print_diff(iter, "+ ", &Style::new().green());
}

/// Print remove iterator.
pub fn remove<T>(iter: T)
where
    T: IntoIterator,
    T::Item: fmt::Display,
{
    print_diff(iter, "- ", &Style::new().red());
}

/// Print formatted diff for files.
pub fn diff_files<T, U>(original: T, modified: U)
where
    T: std::string::ToString,
    U: std::string::ToString,
{
    if log_enabled!(target: "diff", log::Level::Info) {
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
}

fn format_change<T: similar::DiffableStr + ?Sized>(change: Change<&T>) -> String {
    match change.tag() {
        ChangeTag::Equal => format!("  {change}"),
        ChangeTag::Delete => Style::new()
            .red()
            .apply_to(format!("- {s}", s = change,))
            .to_string(),
        ChangeTag::Insert => Style::new()
            .green()
            .apply_to(format!("+ {s}", s = change,))
            .to_string(),
    }
}

/// Print formatted diff.
pub fn diff<T, U>(original: T, modified: U)
where
    T: std::string::ToString,
    U: std::string::ToString,
{
    if log_enabled!(target: "diff", log::Level::Info) {
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
}

fn ansible_log_format(out: FormatCallback, message: &fmt::Arguments, record: &log::Record) {
    let level = record.level();
    let target = record.target();
    let log_header = match (level, target) {
        (log::Level::Error, "task") => "failed: ".to_owned(),
        (log::Level::Error, _) => "[ERROR] ".to_owned(),
        (log::Level::Warn, _) => "[WARNING] ".to_owned(),
        (log::Level::Info, "changed" | "changed_empty") => "changed: ".to_owned(),
        (log::Level::Info, "ignoring") => "[ignoring error] ".to_owned(),
        (log::Level::Info, "ok" | "ok_empty") => "ok: ".to_owned(),
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
        (log::Level::Debug, _) => Style::new().color256(COLOR_BRIGHT_BLUE), // bright blue
        (log::Level::Trace, _) => Style::new().color256(COLOR_BRIGHT_BLACK), // bright black
    };
    let line = format!(
        "{log_header}{message}{separator}",
        log_header = log_header,
        message = &message,
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
            (_, _) => "".to_string(),
        },
    );
    out.finish(format_args!("{}", style.apply_to(line)))
}

fn raw_log_format(out: FormatCallback, message: &fmt::Arguments, _record: &log::Record) {
    out.finish(format_args!("{message}"))
}

/// Setup logging according to the specified verbosity.
pub fn setup_logging(verbosity: u8, diff: &bool, output: &Output) -> Result<()> {
    let mut base_config = fern::Dispatch::new();

    base_config = match verbosity {
        0 => base_config.level(log::LevelFilter::Info),
        1 => base_config.level(log::LevelFilter::Debug),
        _2_or_more => base_config.level(log::LevelFilter::Trace),
    };

    base_config = match diff {
        false => base_config.level_for("diff", log::LevelFilter::Error),
        true => base_config.level_for("diff", log::LevelFilter::Info),
    };

    // remove task module for raw output
    base_config = match output {
        Output::Raw => base_config.level_for("task", log::LevelFilter::Error),
        _ => base_config,
    };

    let log_format = match output {
        Output::Ansible => ansible_log_format,
        Output::Raw => raw_log_format,
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
