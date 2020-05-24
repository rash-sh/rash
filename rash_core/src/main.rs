use rash_core::context::Context;
use rash_core::error::{Error, ErrorKind};
use rash_core::facts::builtin::Builtins;
use rash_core::facts::env;
use rash_core::logger;
use rash_core::task::read_file;

use std::path::Path;
use std::process::exit;

use clap::{crate_description, crate_version, Clap};

#[macro_use]
extern crate log;

/// Parse a single KEY=VALUE pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn std::error::Error>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + 'static,
    U: std::str::FromStr,
    U::Err: std::error::Error + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

#[derive(Clap)]
#[clap(
    name="rash",
    about = crate_description!(),
    version = crate_version!(),
    author = "Alexander Gil <pando855@gmail.com>",
)]
struct Opts {
    /// Script file to be executed
    script_file: String,
    /// Verbose mode (-vv for more)
    #[clap(short, long, parse(from_occurrences))]
    verbose: u8,
    /// Set environment variables (Example: KEY=VALUE)
    #[clap(short, long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    environment: Vec<(String, String)>,
    /// Additional args to be accessible from builtin `{{ rash.args }}` as list of strings
    #[clap(last = true, multiple = true, takes_value = true)]
    _args: Option<String>,
}

/// Fail program printing [`Error`] and returning code associated if exists.
/// By default fail with `exit(1)`
///
/// [`Error`]: ../rash_core/error/struct.Error.html
fn crash_error(e: Error) {
    error!("{}", e);
    trace!(target: "error", "{:?}", e);
    exit(e.raw_os_error().unwrap_or(1))
}

fn main() {
    let opts: Opts = Opts::parse();

    logger::setup_logging(opts.verbose).expect("failed to initialize logging.");
    trace!("start logger");

    let script_path = Path::new(&opts.script_file);
    match read_file(script_path.to_path_buf()) {
        Ok(tasks) => match Context::exec(Context::new(tasks, {
            let mut facts = env::load(opts.environment).unwrap();
            facts.insert(
                "rash",
                &Builtins::new(
                    opts._args
                        .unwrap_or_else(|| "".to_string())
                        .split(' ')
                        .collect::<Vec<&str>>(),
                    script_path.parent().unwrap_or_else(|| Path::new("/")),
                )
                .unwrap(),
            );
            trace!("Facts: {}", &facts.clone().into_json().to_string());
            facts
        })) {
            Ok(_) => (),
            Err(context_error) => match context_error.kind() {
                ErrorKind::EmptyTaskStack => (),
                _ => crash_error(context_error),
            },
        },
        Err(e) => crash_error(e),
    }
}
