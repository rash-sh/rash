use std::env;
use std::io;
use std::io::Write;
use std::process;

use chrono::Local;
use clap::{Arg, ArgMatches, Command, crate_authors, crate_description, crate_version};
use env_logger::Builder;
use log::LevelFilter;
use mdbook::errors::Error;
use mdbook::preprocess::CmdPreprocessor;

#[macro_use]
extern crate log;

pub fn make_app() -> Command {
    Command::new("mdbook-rash")
        .about(crate_description!())
        .version(crate_version!())
        .author(crate_authors!())
        .subcommand(
            Command::new("supports")
                .arg(Arg::new("renderer").required(true))
                .about("Check whether a renderer is supported by this preprocessor"),
        )
}

fn init_logger() {
    let mut builder = Builder::new();

    builder.format(|formatter, record| {
        writeln!(
            formatter,
            "{} [{}] ({}): {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.target(),
            record.args()
        )
    });

    if let Ok(var) = env::var("RUST_LOG") {
        builder.parse_filters(&var);
    } else {
        // if no RUST_LOG provided, default to logging at the Info level
        builder.filter(None, LevelFilter::Info);
        // Filter extraneous html5ever not-implemented messages
        builder.filter(Some("html5ever"), LevelFilter::Error);
    }

    builder.init();
}

fn main() {
    init_logger();

    let matches = make_app().get_matches();

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        handle_supports(sub_args);
    } else if let Err(e) = handle_preprocessing() {
        error!("{e}");
        process::exit(1);
    }
}

fn handle_preprocessing() -> Result<(), Error> {
    let (ctx, book) = CmdPreprocessor::parse_input(io::stdin()).expect("Invalid book input.");
    let calling_ver = semver::Version::parse(&ctx.mdbook_version).unwrap();
    let library_ver = semver::Version::parse(mdbook::MDBOOK_VERSION).unwrap();

    if calling_ver != library_ver {
        error!(
            "Warning: The mdbook-rash plugin was built against version {} of mdbook, \
            but we're being called from version {}",
            mdbook::MDBOOK_VERSION,
            ctx.mdbook_version
        );
    }

    let processed_book = mdbook_rash::run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;
    Ok(())
}

fn handle_supports(sub_args: &ArgMatches) -> ! {
    let renderer = sub_args
        .get_one::<String>("renderer")
        .expect("Required argument");

    if mdbook_rash::SUPPORTED_RENDERER.contains(&renderer.as_str()) {
        process::exit(0);
    } else {
        process::exit(1);
    }
}
