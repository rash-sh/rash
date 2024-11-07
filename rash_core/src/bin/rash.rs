use rash_core::context::{Context, GlobalParams};
use rash_core::docopt;
use rash_core::error::{Error, ErrorKind};
use rash_core::logger;
use rash_core::task::parse_file;
use rash_core::vars::builtin::Builtins;
use rash_core::vars::env;

use std::error::Error as StdError;
use std::fs::read_to_string;
use std::path::Path;
use std::process::exit;

use clap::error::ErrorKind as ClapErrorKind;
use clap::{crate_authors, crate_description, crate_version, ArgAction, CommandFactory, Parser};
use minijinja::{context, Value};

#[macro_use]
extern crate log;

// Since Rust no longer uses jemalloc by default, rash will, by default,
// use the system allocator. On Linux, this would normally be glibc's
// allocator, which is pretty good. In particular, rash does not have a
// particularly allocation heavy workload, so there really isn't much
// difference (for rash's purposes) between glibc's allocator and jemalloc.
//
// However, when rash is built with musl, this means rash will use musl's
// allocator, which appears to be substantially worse. (musl's goal is not to
// have the fastest version of everything. Its goal is to be small and amenable
// to static compilation.) Even though rash isn't particularly allocation
// heavy, musl's allocator appears to slow down rash quite a bit. Therefore,
// when building with musl, we use jemalloc.
//
// We don't unconditionally use jemalloc because it can be nice to use the
// system's default allocator by default. Moreover, jemalloc seems to increase
// compilation times by a bit.
//
// Moreover, we only do this on 64-bit systems since jemalloc doesn't support
// i686.
#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

/// Parse a single KEY=VALUE pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn std::error::Error + Send + Sync>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + 'static + Send + Sync,
    U: std::str::FromStr,
    U::Err: std::error::Error + 'static + Send + Sync,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

#[derive(Parser, Debug)]
#[command(
    name="rash",
    about = crate_description!(),
    version = crate_version!(),
    author = crate_authors!("\n"),
)]
struct Cli {
    /// run operations with become (does not imply password prompting)
    #[arg(short, long)]
    r#become: bool,
    /// run operations as this user (just works with become enabled)
    #[arg(short='u', long, default_value=GlobalParams::default().become_user)]
    become_user: String,
    /// Execute in dry-run mode without modifications
    #[arg(short, long)]
    check: bool,
    /// Show the differences
    #[arg(short, long)]
    diff: bool,
    /// Set environment variables (Example: KEY=VALUE)
    /// It can be accessed from builtin `{{ env }}`. E.g.: `{{ env.USER }}`
    #[arg(short, long, action = ArgAction::Append, value_parser = parse_key_val::<String, String>, num_args = 1)]
    environment: Vec<(String, String)>,
    /// Output format.
    #[arg(value_enum, short, long, default_value_t=logger::Output::Ansible)]
    output: logger::Output,
    /// Verbose mode (-vv for more)
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,
    /// Inline script to be executed.
    /// If provided, <SCRIPT_FILE> will be used as filename in `rash.path` builtin.
    #[arg(short, long)]
    script: Option<String>,
    /// Path to the script file to be executed.
    /// If provided, this file will be read and used as the script content.
    script_file: Option<String>,
    /// Additional args to be accessible rash scripts.
    ///
    /// It can be accessed from builtin `{{ rash.args }}` as list of strings or if usage is defined
    /// they will be parsed and added as variables too. For more information check rash_book.
    #[arg(action = ArgAction::Append, num_args = 1)]
    script_args: Vec<String>,
}

/// Log all errors recursively
fn log_inner_errors(e: &dyn StdError) {
    error!("{}", e);
    if let Some(source_error) = e.source() {
        log_inner_errors(source_error)
    }
}

/// End the program with failure, printing [`Error`] and returning code associated if exists.
/// By default fail with `exit(1)`
///
/// [`Error`]: ../rash_core/error/struct.Error.html
fn crash_error(e: Error) {
    error!("{}", e);
    let exit_code = e.raw_os_error().unwrap_or(1);

    if let Some(inner_error) = e.into_inner() {
        if let Some(source_error) = inner_error.source() {
            log_inner_errors(source_error)
        }
    }
    exit(exit_code)
}

fn main() {
    let cli: Cli = Cli::parse();
    if cli.script.is_none() && cli.script_file.is_none() {
        let mut cmd = Cli::command();
        cmd.error(
            ClapErrorKind::ArgumentConflict,
            "Please provide either <SCRIPT_FILE> or --script.",
        )
        .exit();
    };
    let verbose = if cli.verbose == 0 {
        match std::env::var("RASH_LOG_LEVEL") {
            Ok(s) => match s.as_ref() {
                "DEBUG" => 1,
                "TRACE" => 2,
                _ => 0,
            },
            _ => 0,
        }
    } else {
        cli.verbose
    };

    logger::setup_logging(verbose, &cli.diff, &cli.output).expect("failed to initialize logging.");
    trace!("start logger");
    trace!("{:?}", &cli);
    let script_path_string = cli.script_file.unwrap_or_else(|| "rash".to_string());
    let script_path = Path::new(&script_path_string);
    let main_file = if let Some(s) = cli.script {
        s
    } else {
        trace!("reading tasks from: {:?}", script_path);
        match read_to_string(script_path) {
            Ok(s) => s,
            Err(e) => return crash_error(Error::new(ErrorKind::InvalidData, e)),
        }
    };

    let script_args: Vec<&str> = cli.script_args.iter().map(|s| &**s).collect();
    let mut new_vars = match docopt::parse(&main_file, &script_args) {
        Ok(v) => Value::from_serialize(v),
        Err(e) => match e.kind() {
            ErrorKind::GracefulExit => {
                info!("{}", e);
                return;
            }
            _ => return crash_error(e),
        },
    };

    let global_params = GlobalParams {
        r#become: cli.r#become,
        become_user: &cli.become_user,
        check_mode: cli.check,
    };

    match parse_file(&main_file, &global_params) {
        Ok(tasks) => {
            let env_vars = env::load(cli.environment);
            new_vars = context! {..new_vars, ..env_vars};
            match Builtins::new(
                script_args.into_iter().map(String::from).collect(),
                script_path,
            ) {
                Ok(builtins) => new_vars = context! {rash => &builtins, ..new_vars},
                Err(e) => crash_error(e),
            };
            trace!("Vars: {new_vars}");
            match Context::new(tasks, new_vars).exec() {
                Ok(_) => (),
                Err(context_error) => match context_error.kind() {
                    ErrorKind::EmptyTaskStack => (),
                    _ => crash_error(context_error),
                },
            };
        }
        Err(e) => crash_error(e),
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
