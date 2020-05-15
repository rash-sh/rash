use rash_core::context::Context;
use rash_core::error::ErrorKind;
use rash_core::facts::FACTS_SOURCES;
use rash_core::input::read_file;
use rash_core::logger;

use std::path::PathBuf;
use std::process::exit;

use clap::{crate_version, Clap};

#[macro_use]
extern crate log;

/// Declarative shell using Rust native bindings scripting
#[derive(Clap)]
#[clap(version = crate_version!(), author = "Alexander Gil <pando855@gmail.com>")]
struct Opts {
    /// Script file to be executed
    script_file: String,
    /// Verbose mode (-vv for more)
    #[clap(short, long, parse(from_occurrences))]
    verbose: u8,
}

fn main() {
    let opts: Opts = Opts::parse();

    logger::setup_logging(opts.verbose).expect("failed to initialize logging.");
    trace!("start logger");
    let facts_fn = FACTS_SOURCES.get("env").unwrap();
    let context = Context::new(
        read_file(PathBuf::from(opts.script_file)).unwrap(),
        (facts_fn)().unwrap(),
    );
    let context_error = Context::exec(context).unwrap_err();
    let _ = match context_error.kind() {
        ErrorKind::EmptyTaskStack => (),
        _ => {
            error!("{}", context_error);
            trace!(target: "error", "{:?}", context_error);
            exit(1)
        }
    };
}
