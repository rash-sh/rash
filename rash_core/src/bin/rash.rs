use rash_core::context::{BecomeMethod, Context, GlobalParams};
use rash_core::docopt;
use rash_core::error::{Error, ErrorKind};
use rash_core::logger;
use rash_core::modules::add_module_search_path;
use rash_core::task::{
    InternalTaskData, get_internal_result_path, parse_file, parse_file_with_handlers,
};
use rash_core::vars::builtin::Builtins;
use rash_core::vars::env;

use rpassword::read_password;
use std::error::Error as StdError;
use std::fs::{File, read_to_string};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

use clap::error::ErrorKind as ClapErrorKind;
use clap::{ArgAction, CommandFactory, Parser, crate_authors, crate_description, crate_version};
use minijinja::{Value, context};

#[macro_use]
extern crate log;

#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

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
    /// Privilege escalation method to use (syscall or sudo)
    #[arg(long, value_enum, default_value_t=BecomeMethod::default())]
    become_method: BecomeMethod,
    /// Path to sudo executable (used when become_method is sudo)
    #[arg(long, default_value=GlobalParams::default().become_exe)]
    become_exe: String,
    /// Ask for privilege escalation password
    #[arg(short = 'K', long)]
    ask_become_pass: bool,
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
    /// Internal task file for sudo become execution (hidden, not for direct use)
    #[arg(long, hide = true)]
    internal_task: Option<PathBuf>,
}

fn log_inner_errors(e: &dyn StdError) {
    error!("{e}");
    if let Some(source_error) = e.source() {
        log_inner_errors(source_error)
    }
}

fn crash_error(e: Error) -> ! {
    error!("{e}");
    let exit_code = e.raw_os_error().unwrap_or(1);

    if let Some(inner_error) = e.into_inner()
        && let Some(source_error) = inner_error.source()
    {
        log_inner_errors(source_error)
    }
    exit(exit_code)
}

fn setup_module_search_paths(script_path: &Path) {
    if let Some(script_dir) = script_path.parent() {
        let script_modules = script_dir.join("modules");
        if script_modules.exists() {
            trace!(
                "Adding script-relative module search path: {:?}",
                script_modules
            );
            add_module_search_path(script_modules);
        }
    }

    let system_modules = PathBuf::from("/etc/rash/modules");
    if system_modules.exists() {
        trace!("Adding system module search path: {:?}", system_modules);
        add_module_search_path(system_modules);
    }

    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        let user_modules = PathBuf::from(config_home).join("rash").join("modules");
        if user_modules.exists() {
            trace!("Adding user module search path (XDG): {:?}", user_modules);
            add_module_search_path(user_modules);
        }
    } else if let Ok(home) = std::env::var("HOME") {
        let user_modules = PathBuf::from(home)
            .join(".config")
            .join("rash")
            .join("modules");
        if user_modules.exists() {
            trace!("Adding user module search path (HOME): {:?}", user_modules);
            add_module_search_path(user_modules);
        }
    }
}

fn execute_internal_task(task_path: &Path) {
    trace!("Internal task execution from: {:?}", task_path);

    let task_content = match read_to_string(task_path) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to read internal task file: {}", e);
            exit(1);
        }
    };

    let internal_data: InternalTaskData = match serde_yaml::from_str(&task_content) {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to parse internal task data: {}", e);
            exit(1);
        }
    };

    let global_params = GlobalParams::default();

    let task_yaml =
        serde_yaml::to_string(std::slice::from_ref(&internal_data.task)).unwrap_or_default();

    let (tasks, _) = match parse_file_with_handlers(&task_yaml, &global_params) {
        Ok(parsed) => (parsed.tasks, parsed.handlers),
        Err(_) => match parse_file(&task_yaml, &global_params) {
            Ok(tasks) => (tasks, None),
            Err(e) => {
                error!("Failed to parse internal task: {}", e);
                exit(1);
            }
        },
    };

    let script_path = internal_data
        .original_path
        .as_deref()
        .map(Path::new)
        .unwrap_or_else(|| Path::new("internal_task"));

    let result = match Builtins::new(internal_data.args.unwrap_or_default(), script_path, false) {
        Ok(builtins) => {
            let vars = context! {rash => &builtins, ..internal_data.vars};
            trace!("Internal task vars: {:?}", vars);
            Context::new(tasks, vars, None).exec()
        }
        Err(e) => {
            error!("Failed to create builtins: {}", e);
            exit(1);
        }
    };

    let result_path = match get_internal_result_path() {
        Some(p) => p,
        None => {
            error!("No result file path specified");
            exit(1);
        }
    };

    match result {
        Ok(_context) => {
            let exec_result = rash_core::task::TaskExecResult::new(false, None);
            let result_json = serde_json::to_string(&exec_result).unwrap_or_default();
            if let Err(e) =
                File::create(&result_path).and_then(|mut f| f.write_all(result_json.as_bytes()))
            {
                error!("Failed to write result file: {}", e);
                exit(1);
            }
        }
        Err(e) => {
            error!("Internal task failed: {}", e);
            exit(1);
        }
    }
}

fn main() {
    let cli: Cli = Cli::parse();

    // Handle internal task execution for sudo become
    if let Some(internal_task_path) = &cli.internal_task {
        execute_internal_task(internal_task_path);
        return;
    }

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

    setup_module_search_paths(script_path);

    let main_file = if let Some(s) = cli.script {
        s
    } else {
        trace!("reading tasks from: {script_path:?}");
        match read_to_string(script_path) {
            Ok(s) => s,
            Err(e) => crash_error(Error::new(ErrorKind::InvalidData, e)),
        }
    };

    let script_args: Vec<&str> = cli.script_args.iter().map(|s| &**s).collect();
    let mut new_vars = match docopt::parse(&main_file, &script_args) {
        Ok(v) => Value::from_serialize(v),
        Err(e) => match e.kind() {
            ErrorKind::GracefulExit => {
                info!("{e}");
                return;
            }
            _ => crash_error(e),
        },
    };

    let global_params = GlobalParams {
        r#become: cli.r#become,
        become_user: &cli.become_user,
        become_method: cli.become_method,
        become_exe: &cli.become_exe,
        become_password: if cli.ask_become_pass {
            // Prompt for password
            eprint!("BECOME password: ");
            let password = read_password().unwrap_or_default();
            Some(password.leak() as &'static str)
        } else {
            None
        },
        check_mode: cli.check,
    };

    let (tasks, handlers) = match parse_file_with_handlers(&main_file, &global_params) {
        Ok(parsed) => (parsed.tasks, parsed.handlers),
        Err(e1) => match parse_file(&main_file, &global_params) {
            Ok(tasks) => (tasks, None),
            Err(_) => {
                crash_error(e1);
            }
        },
    };

    let env_vars = env::load(cli.environment);
    new_vars = context! {..new_vars, ..env_vars};
    match Builtins::new(
        script_args.into_iter().map(String::from).collect(),
        script_path,
        cli.check,
    ) {
        Ok(builtins) => new_vars = context! {rash => &builtins, ..new_vars},
        Err(e) => crash_error(e),
    };
    trace!("Vars: {new_vars}");
    match Context::with_handlers(tasks, new_vars, None, handlers).exec() {
        Ok(_) => (),
        Err(context_error) => match context_error.kind() {
            ErrorKind::EmptyTaskStack => (),
            _ => crash_error(context_error),
        },
    };
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
