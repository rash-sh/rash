use rash_core::context::Context;
use rash_core::error::{Error, ErrorKind};
use rash_core::facts::FACTS_SOURCES;
use rash_core::logger;
use rash_core::task::read_file;

use std::path::PathBuf;
use std::process::exit;

use clap::{crate_version, Clap};

#[macro_use]
extern crate log;

/// Declarative shell scripting using Rust native bindings
#[derive(Clap)]
#[clap(version = crate_version!(), author = "Alexander Gil <pando855@gmail.com>")]
struct Opts {
    /// Script file to be executed
    script_file: String,
    /// Verbose mode (-vv for more)
    #[clap(short, long, parse(from_occurrences))]
    verbose: u8,
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
    let facts_fn = FACTS_SOURCES.get("env").unwrap();

    match read_file(PathBuf::from(opts.script_file)) {
        Ok(tasks) => match Context::exec(Context::new(tasks, (facts_fn)().unwrap())) {
            Ok(_) => (),
            Err(context_error) => match context_error.kind() {
                ErrorKind::EmptyTaskStack => (),
                _ => crash_error(context_error),
            },
        },
        Err(e) => crash_error(e),
    }
}
