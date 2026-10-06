//! Privilege escalation (`become`) for tasks.
//!
//! A module runs as another user in a `rash --internal-task` child process, started either
//! directly (`syscall`: the child switches user itself before running the task) or through
//! sudo (`sudo`). Both methods share one protocol: the task goes to the child in a private
//! temporary file, its outcome comes back in another one, and the child is supervised like
//! any foreground process, so signals are forwarded to it and `always` sections still run.
//! The child is a fresh process image: unlike a bare `fork`, it cannot inherit locks held
//! by other threads of the parent.
use crate::context::{BecomeMethod, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::process::{OutputMode, ProcessResult, ProcessSpec};
use crate::task::{Task, TaskExecResult};

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};

use minijinja::Value;
use nix::unistd::{Gid, Uid, User, fchown, setgid, setuid};
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;
use tempfile::NamedTempFile;

/// Task sent to a become child process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTaskData {
    pub vars: Value,
    pub task: YamlValue,
    /// User the child switches to before running the task (`syscall` method).
    #[serde(default)]
    pub user: Option<BecomeUser>,
}

/// A become user, resolved by the parent Rash process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BecomeUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
}

impl From<&User> for BecomeUser {
    fn from(user: &User) -> Self {
        Self {
            name: user.name.clone(),
            uid: user.uid.as_raw(),
            gid: user.gid.as_raw(),
        }
    }
}

impl BecomeUser {
    /// Switch the current process to this user, with the user's supplementary groups
    /// instead of the caller's ones (like a login).
    pub fn switch(&self) -> Result<()> {
        let failed = |what: &str, error: &dyn std::fmt::Display| {
            Error::new(
                ErrorKind::Other,
                format!(
                    "cannot become user {}: {what} cannot be changed: {error}",
                    self.name
                ),
            )
        };
        let gid = Gid::from_raw(self.gid);
        set_supplementary_groups(&self.name, gid).map_err(|e| failed("groups", &e))?;
        setgid(gid).map_err(|e| failed("gid", &e))?;
        setuid(Uid::from_raw(self.uid)).map_err(|e| failed("uid", &e))
    }
}

#[cfg(not(any(target_vendor = "apple", target_os = "redox", target_os = "haiku")))]
fn set_supplementary_groups(name: &str, gid: Gid) -> Result<()> {
    let name = std::ffi::CString::new(name).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    nix::unistd::initgroups(&name, gid)?;
    Ok(())
}

/// No `initgroups(3)` binding here: keep only the primary group, never the caller's groups.
#[cfg(any(target_vendor = "apple", target_os = "redox", target_os = "haiku"))]
fn set_supplementary_groups(_name: &str, gid: Gid) -> Result<()> {
    let groups = [gid.as_raw()];
    // SAFETY: setgroups(2) reads exactly one gid from a live array.
    if unsafe { libc::setgroups(1, groups.as_ptr()) } != 0 {
        return Err(Error::new(
            ErrorKind::Other,
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

pub const RASH_INTERNAL_TASK_ENV: &str = "RASH_INTERNAL_TASK_FILE";
pub const RASH_INTERNAL_RESULT_ENV: &str = "RASH_INTERNAL_RESULT_FILE";
pub const RASH_INTERNAL_OUTPUT_ENV: &str = "RASH_INTERNAL_OUTPUT";
pub const RASH_INTERNAL_TASK_FLAG: &str = "RASH_INTERNAL";

/// Termination signals a become child turns into its exit status (`128 + signal`).
const CHILD_INTERRUPT_SIGNALS: [i32; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

pub fn is_internal_task_execution() -> Option<PathBuf> {
    env::var(RASH_INTERNAL_TASK_ENV).ok().map(PathBuf::from)
}

pub fn get_internal_result_path() -> Option<PathBuf> {
    env::var(RASH_INTERNAL_RESULT_ENV).ok().map(PathBuf::from)
}

pub fn get_internal_output() -> Option<String> {
    env::var(RASH_INTERNAL_OUTPUT_ENV).ok()
}

pub fn is_internal_execution() -> bool {
    env::var(RASH_INTERNAL_TASK_FLAG).is_ok()
}

/// Outcome of a task executed in a become child process, sent back to the parent so that
/// explicit exits, interrupts and errors keep their meaning instead of becoming a generic
/// child failure.
#[derive(Debug, Serialize, Deserialize)]
pub enum BecomeOutcome {
    Done(TaskExecResult),
    Exit(i32),
    Interrupted(i32),
    Failed(String),
}

impl From<Result<TaskExecResult>> for BecomeOutcome {
    fn from(result: Result<TaskExecResult>) -> Self {
        match result {
            Ok(result) => Self::Done(result),
            Err(error) => match (error.kind(), error.raw_os_error()) {
                (ErrorKind::ExplicitExit, Some(code)) => Self::Exit(code),
                (ErrorKind::Interrupted, Some(code)) => Self::Interrupted(code - 128),
                _ => Self::Failed(error.to_string()),
            },
        }
    }
}

impl BecomeOutcome {
    pub fn into_result(self) -> Result<TaskExecResult> {
        match self {
            Self::Done(result) => Ok(result),
            Self::Exit(code) => Err(Error::explicit_exit(code)),
            Self::Interrupted(signal) => Err(Error::interrupted(signal)),
            Self::Failed(message) => Err(Error::new(ErrorKind::Other, message)),
        }
    }

    fn from_json(json: &str) -> Result<TaskExecResult> {
        serde_json::from_str::<Self>(json)
            .map_err(|e| {
                Error::new(
                    ErrorKind::Other,
                    format!("Failed to parse become result: {e}"),
                )
            })?
            .into_result()
    }
}

/// Body of `rash --internal-task`: run the task of a become child and write its outcome
/// to the result file. Errors, explicit exits and interrupts of the task are part of the
/// outcome; an error is returned only when no outcome can be reported.
pub fn execute_internal_task(task_path: &Path) -> Result<()> {
    let content = fs::read_to_string(task_path).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to read internal task file: {e}"),
        )
    })?;
    let data: InternalTaskData = serde_yaml::from_str(&content).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse internal task data: {e}"),
        )
    })?;
    // Opened before switching user: the parent created it private to its own user.
    let mut result_file = open_result_file()?;
    let outcome = BecomeOutcome::from(run_internal_task(data));
    let json = serde_json::to_string(&outcome).map_err(|e| Error::new(ErrorKind::Other, e))?;
    result_file.write_all(json.as_bytes()).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to write result file: {e}"),
        )
    })
}

fn open_result_file() -> Result<File> {
    let path = get_internal_result_path()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "No result file path specified"))?;
    // Never create a result file or follow a symlink.
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to open result file: {e}"),
            )
        })
}

fn run_internal_task(data: InternalTaskData) -> Result<TaskExecResult> {
    if let Some(user) = &data.user {
        user.switch()?;
    }
    let global_params = GlobalParams::default();
    Task::new(&data.task, &global_params)?.exec_in_become_child(data.vars)
}

fn path_arg(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("{path:?} cannot be represented as UTF-8"),
        )
    })
}

/// Logging flags of this process, so that a become child logs like its parent.
fn log_flags() -> Vec<String> {
    let mut flags = Vec::new();
    if log_enabled!(log::Level::Trace) {
        flags.push("-vv".to_owned());
    } else if log_enabled!(log::Level::Debug) {
        flags.push("-v".to_owned());
    }
    if log_enabled!(target: "diff", log::Level::Info) {
        flags.push("--diff".to_owned());
    }
    flags
}

impl Task {
    /// Control-flow modules (block, include, meta...) never run as another user: they have no
    /// side effects of their own and must act on the Rash process (exit, scoped vars). Child
    /// tasks inherit the become settings and escalate on their own.
    pub(super) fn runs_with_become(&self) -> bool {
        self.r#become && !self.check_mode && !self.module.is_control_flow()
    }

    pub(super) fn exec_module_with_become(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
    ) -> Result<TaskExecResult> {
        if self.become_method == BecomeMethod::Syscall {
            let user = self.resolve_become_user()?;
            if user.uid == Uid::current() {
                return self.execute_module_with_environment(rendered_params, vars);
            }
            if self.transfers_pid(rendered_params) {
                // The process is replaced on success: switching users in place is fine.
                BecomeUser::from(&user).switch()?;
                return self.execute_module_with_environment(rendered_params, vars);
            }
        }
        self.exec_module_in_become_child(rendered_params, vars)
    }

    fn transfers_pid(&self, rendered_params: &YamlValue) -> bool {
        self.module.get_name() == "command"
            && rendered_params
                .get("transfer_pid")
                .and_then(YamlValue::as_bool)
                .unwrap_or(false)
    }

    pub(super) fn resolve_become_user(&self) -> Result<User> {
        let not_found = || {
            Error::new(
                ErrorKind::Other,
                format!("User {:?} not found", self.become_user),
            )
        };
        if let Some(user) = User::from_name(&self.become_user).map_err(|_| not_found())? {
            return Ok(user);
        }
        let uid = self
            .become_user
            .parse::<u32>()
            .map(Uid::from_raw)
            .map_err(|_| not_found())?;
        User::from_uid(uid)?.ok_or_else(not_found)
    }

    fn internal_task(&self, rendered_params: &YamlValue) -> YamlValue {
        let mut mapping = serde_norway::Mapping::new();
        let key = |name: &str| YamlValue::String(name.to_owned());
        mapping.insert(key(self.module.get_name()), rendered_params.clone());
        let strings = [
            ("name", &self.name),
            ("changed_when", &self.changed_when),
            ("failed_when", &self.failed_when),
            ("register", &self.register),
        ];
        for (name, value) in strings {
            if let Some(value) = value {
                mapping.insert(key(name), YamlValue::String(value.clone()));
            }
        }
        if let Some(environment) = &self.environment {
            mapping.insert(key("environment"), environment.clone());
        }
        if self.quiet {
            mapping.insert(key("quiet"), YamlValue::Bool(true));
        }
        if self.no_log {
            mapping.insert(key("no_log"), YamlValue::Bool(true));
        }
        YamlValue::Mapping(mapping)
    }

    fn internal_task_data(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
    ) -> Result<InternalTaskData> {
        let user = match self.become_method {
            BecomeMethod::Syscall => Some(BecomeUser::from(&self.resolve_become_user()?)),
            // sudo already starts the child as the become user.
            BecomeMethod::Sudo => None,
        };
        Ok(InternalTaskData {
            vars: self.extend_vars(vars.clone())?,
            task: self.internal_task(rendered_params),
            user,
        })
    }

    /// Create a temporary file only the current user and the become child can access. Task
    /// files hold rendered params and vars, which may include secrets.
    fn private_file(&self, prefix: &str) -> Result<NamedTempFile> {
        // Created with O_EXCL, a random name and mode 0600; removed on drop.
        let file = tempfile::Builder::new()
            .prefix(prefix)
            .tempfile()
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        // The syscall child opens both files before switching user.
        if self.become_method == BecomeMethod::Syscall {
            return Ok(file);
        }
        let user = self.resolve_become_user()?;
        let current = Uid::effective();
        if user.uid.is_root() || user.uid == current {
            return Ok(file);
        }
        if !current.is_root() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "become_method sudo cannot share task data privately with user {:?}: run \
                     rash as root or become root instead",
                    self.become_user
                ),
            ));
        }
        fchown(file.as_file(), Some(user.uid), Some(user.gid))?;
        Ok(file)
    }

    fn sudo_spec(&self, rash: String, internal_args: Vec<String>) -> ProcessSpec {
        let mut spec = ProcessSpec::new(&self.become_exe);
        spec.args = ["-H", "-E", "-u", &self.become_user]
            .map(str::to_owned)
            .to_vec();
        spec.stderr = OutputMode::Inherit;
        if let Some(password) = &self.become_password {
            spec.args.push("-S".to_owned());
            spec.stdin = Some(format!("{password}\n"));
            // Keep the password prompt off the terminal; it is reported on failure.
            spec.stderr = OutputMode::Capture;
        }
        spec.args.push("--".to_owned());
        spec.args.push(rash);
        spec.args.extend(internal_args);
        spec
    }

    fn become_child_spec(&self, task_file: &Path, result_file: &Path) -> Result<ProcessSpec> {
        let rash = env::current_exe().map_err(|e| Error::new(ErrorKind::Other, e))?;
        let rash = path_arg(&rash)?;
        let mut internal_args = vec!["--internal-task".to_owned(), path_arg(task_file)?];
        internal_args.extend(log_flags());
        let mut spec = match self.become_method {
            BecomeMethod::Sudo => self.sudo_spec(rash, internal_args),
            BecomeMethod::Syscall => {
                let mut spec = ProcessSpec::new(rash);
                spec.args = internal_args;
                spec.stderr = OutputMode::Inherit;
                spec
            }
        };
        spec.stdout = OutputMode::Inherit;
        spec.env = vec![
            (RASH_INTERNAL_RESULT_ENV.to_owned(), path_arg(result_file)?),
            (RASH_INTERNAL_TASK_FLAG.to_owned(), "1".to_owned()),
        ];
        Ok(spec)
    }

    fn become_launcher(&self) -> &str {
        match self.become_method {
            BecomeMethod::Sudo => &self.become_exe,
            BecomeMethod::Syscall => "become child",
        }
    }

    /// Error of a become child that exited without reporting an outcome.
    fn become_child_error(&self, output: &ProcessResult) -> Error {
        // A child stopped by a signal outside a supervised process exits with 128 + signal.
        if let Some(signal) = output.status.code().map(|code| code - 128)
            && CHILD_INTERRUPT_SIGNALS.contains(&signal)
        {
            return Error::interrupted(signal);
        }
        let status = match output.status.signal() {
            Some(signal) => format!("was killed by signal {signal}"),
            None => format!("failed with exit code {}", output.rc()),
        };
        let stderr = output.stderr.as_deref().unwrap_or_default().trim();
        let detail = if stderr.is_empty() {
            String::new()
        } else {
            format!(": {stderr}")
        };
        Error::new(
            ErrorKind::SubprocessFail,
            format!("{} {status}{detail}", self.become_launcher()),
        )
    }

    fn exec_module_in_become_child(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
    ) -> Result<TaskExecResult> {
        // Both files are removed when dropped, also on errors and interrupts.
        let mut task_file = self.private_file("rash_task_")?;
        let result_file = self.private_file("rash_result_")?;
        let task_content = serde_yaml::to_string(&self.internal_task_data(rendered_params, vars)?)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        task_file
            .write_all(task_content.as_bytes())
            .and_then(|()| task_file.flush())
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;

        let spec = self.become_child_spec(task_file.path(), result_file.path())?;
        // Supervised as a foreground child: a signal is forwarded to it and, once it is
        // reaped, becomes an interrupt error.
        let output = spec.run().map_err(|e| {
            if e.is_termination() {
                return e;
            }
            Error::new(
                ErrorKind::SubprocessFail,
                format!("{} cannot be started: {e}", self.become_launcher()),
            )
        })?;
        if !output.success() {
            return Err(self.become_child_error(&output));
        }
        let result_content = fs::read_to_string(result_file.path()).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("Failed to read become result file: {e}"),
            )
        })?;
        BecomeOutcome::from_json(&result_content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GlobalParams;

    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn task_with(yaml: &str, global_params: &GlobalParams) -> Task {
        Task::new(&serde_norway::from_str(yaml).unwrap(), global_params).unwrap()
    }

    #[test]
    fn control_flow_modules_never_run_with_become() {
        let global_params = GlobalParams::default();
        for module in [
            "meta: {action: exit}",
            "block: [{debug: {msg: hi}}]",
            "include: other.rh",
            "set_vars: {a: 1}",
        ] {
            let task = task_with(&format!("{module}\nbecome: true"), &global_params);
            assert!(!task.runs_with_become(), "{module}");
        }
        let task = task_with("command: id\nbecome: true", &global_params);
        assert!(task.runs_with_become());
    }

    #[test]
    fn global_become_does_not_route_meta_exit_through_child() {
        let global_params = GlobalParams {
            r#become: true,
            ..GlobalParams::default()
        };
        let task = task_with("meta: {action: exit, code: 9}", &global_params);
        let error = task.exec(minijinja::context! {}).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ExplicitExit);
        assert_eq!(error.raw_os_error(), Some(9));
    }

    #[test]
    fn become_outcome_round_trips_termination() {
        let cases = [
            (Error::explicit_exit(7), ErrorKind::ExplicitExit, Some(7)),
            (Error::interrupted(15), ErrorKind::Interrupted, Some(143)),
            (Error::new(ErrorKind::Other, "boom"), ErrorKind::Other, None),
        ];
        for (error, kind, code) in cases {
            let json = serde_json::to_string(&BecomeOutcome::from(Err(error))).unwrap();
            let error = BecomeOutcome::from_json(&json).unwrap_err();
            assert_eq!(error.kind(), kind);
            assert_eq!(error.raw_os_error(), code);
        }
        let json = serde_json::to_string(&BecomeOutcome::from(Ok(TaskExecResult::new(true, None))))
            .unwrap();
        assert!(BecomeOutcome::from_json(&json).unwrap().get_changed());
    }

    #[test]
    fn private_file_is_owner_only() {
        let global_params = GlobalParams::default();
        for method in ["syscall", "sudo"] {
            let yaml = format!("debug: {{msg: hi}}\nbecome_user: root\nbecome_method: {method}");
            let task = task_with(&yaml, &global_params);
            let file = task.private_file("rash_test_").unwrap();
            let mode = file.as_file().metadata().unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{method}");
        }
    }

    #[test]
    fn syscall_private_file_needs_no_ownership_change() {
        // The syscall child opens the files before switching user.
        let global_params = GlobalParams::default();
        let task = task_with("debug: {msg: hi}\nbecome_user: nobody", &global_params);
        let file = task.private_file("rash_test_").unwrap();
        assert_eq!(
            file.as_file().metadata().unwrap().uid(),
            Uid::effective().as_raw()
        );
    }

    #[test]
    fn sudo_private_file_refuses_other_user_when_not_root() {
        if Uid::effective().is_root() {
            return;
        }
        let global_params = GlobalParams::default();
        let task = task_with(
            "debug: {msg: hi}\nbecome_user: nobody\nbecome_method: sudo",
            &global_params,
        );
        assert!(task.private_file("rash_test_").is_err());
    }
}
