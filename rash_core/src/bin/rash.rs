use rash_core::context::Context;
use rash_core::docopt;
use rash_core::error::{Error, ErrorKind};
use rash_core::logger;
use rash_core::task::{parse_file, GlobalParams};
use rash_core::vars::builtin::Builtins;
use rash_core::vars::env;

use std::error::Error as StdError;
use std::fs::read_to_string;
use std::path::Path;
use std::process::exit;

use clap::{crate_authors, crate_description, crate_version, ArgAction, Parser};

#[macro_use]
extern crate log;

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
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

#[derive(Parser, Debug)]
#[command(
    name="rash",
    about = crate_description!(),
    version = crate_version!(),
    author = crate_authors!("\n"),
)]
struct Args {
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
    /// Verbose mode (-vv for more)
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,
    /// Script file to be executed
    script_file: String,
    /// Additional args to be accessible rash scripts.
    ///
    /// It can be accessed from builtin `{{ rash.args }}` as list of strings or if usage is defined
    /// they will be parsed and added as variables too. For more information check rash_book.
    #[arg(action = ArgAction::Append, num_args = 1)]
    script_args: Vec<String>,
}

/// Trace all errors recursively
fn trace_all(e: &dyn StdError) {
    trace!(target: "error", "{}", e);
    if let Some(source_error) = e.source() {
        trace_all(source_error)
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
            trace_all(source_error)
        }
    }
    exit(exit_code)
}

fn main() {
    let args: Args = Args::parse();

    let verbose = if args.verbose == 0 {
        match std::env::var("RASH_LOG_LEVEL") {
            Ok(s) => match s.as_ref() {
                "DEBUG" => 1,
                "TRACE" => 2,
                _ => 0,
            },
            _ => 0,
        }
    } else {
        args.verbose
    };

    logger::setup_logging(verbose, args.diff).expect("failed to initialize logging.");
    trace!("start logger");
    trace!("{:?}", &args);
    let script_path = Path::new(&args.script_file);
    trace!("reading tasks from: {:?}", script_path);
    let main_file = match read_to_string(script_path) {
        Ok(s) => s,
        Err(e) => return crash_error(Error::new(ErrorKind::InvalidData, e)),
    };

    let script_args: Vec<&str> = args.script_args.iter().map(|s| &**s).collect();
    let mut new_vars = match docopt::parse(&main_file, &script_args) {
        Ok(v) => v,
        Err(e) => match e.kind() {
            ErrorKind::GracefulExit => {
                info!("{}", e);
                return;
            }
            _ => return crash_error(e),
        },
    };

    let global_params = GlobalParams {
        r#become: args.r#become,
        become_user: &args.become_user,
        check_mode: args.check,
    };

    match parse_file(&main_file, &global_params) {
        Ok(tasks) => match env::load(args.environment) {
            Ok(env_vars) => {
                new_vars.extend(env_vars);
                match Builtins::new(script_args, script_path) {
                    Ok(builtins) => new_vars.insert("rash", &builtins),
                    Err(e) => crash_error(e),
                };
                trace!("Vars: {}", &new_vars.clone().into_json().to_string());
                match Context::exec(Context::new(tasks, new_vars)) {
                    Ok(_) => (),
                    Err(context_error) => match context_error.kind() {
                        ErrorKind::EmptyTaskStack => (),
                        _ => crash_error(context_error),
                    },
                };
            }
            Err(e) => crash_error(e),
        },
        Err(e) => crash_error(e),
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Args::command().debug_assert()
}
