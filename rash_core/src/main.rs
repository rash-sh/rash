use rash_core::context::Context;
use rash_core::error::ErrorKind;
use rash_core::facts::FACTS_SOURCES;
use rash_core::input::read_file;
use rash_core::logger;

use std::path::PathBuf;

#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref TASKS_PATH: PathBuf = PathBuf::from("./entrypoint.rh");
}

fn main() {
    let cmd_arguments = clap::App::new("cmd-program")
        .arg(
            clap::Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("Increases logging verbosity each use for up to 3 times"),
        )
        .get_matches();

    let verbosity = cmd_arguments.occurrences_of("verbose");

    logger::setup_logging(verbosity).expect("failed to initialize logging.");
    trace!("start logger");
    let facts_fn = FACTS_SOURCES.get("env").unwrap();
    let context = Context::new(
        read_file(PathBuf::from("./entrypoint.rh")).unwrap(),
        (facts_fn)().unwrap(),
    );
    let context_error = Context::exec(context).unwrap_err();
    let _ = match context_error.kind() {
        ErrorKind::EmptyTaskStack => (),
        _ => panic!(context_error),
    };
}
