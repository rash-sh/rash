use rash_core::context::Context;
use rash_core::error::{Error, ErrorKind};
use rash_core::facts::env;
use rash_core::logger;
use rash_core::task::read_file;

use std::path::PathBuf;
use std::process::exit;

use clap::{crate_version, Clap};

#[macro_use]
extern crate log;

/// Parse a single key-value pair
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

/// Declarative shell scripting using Rust native bindings
#[derive(Clap)]
#[clap(name="rash", version = crate_version!(), author = "Alexander Gil <pando855@gmail.com>")]
struct Opts {
    /// Script file to be executed
    script_file: String,
    /// Verbose mode (-vv for more)
    #[clap(short, long, parse(from_occurrences))]
    verbose: u8,
    /// Set environment variables (Example: KEY=VALUE)
    #[clap(short, long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    environment: Vec<(String, String)>,
}

fn crash_error(e: Error) {
    error!("{}", e);
    trace!(target: "error", "{:?}", e);
    exit(1)
}

fn main() {
    let opts: Opts = Opts::parse();

    logger::setup_logging(opts.verbose).expect("failed to initialize logging.");
    trace!("start logger");

    match read_file(PathBuf::from(opts.script_file)) {
        Ok(tasks) => match Context::exec(Context::new(tasks, env::load(opts.environment).unwrap()))
        {
            Ok(_) => (),
            Err(context_error) => match context_error.kind() {
                ErrorKind::EmptyTaskStack => (),
                _ => crash_error(context_error),
            },
        },
        Err(e) => crash_error(e),
    }
}
