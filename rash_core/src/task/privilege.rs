//! Privilege escalation (`become`) for tasks: run a module as another user, either in a
//! forked child that switches uid/gid (`syscall`) or in a `rash --internal-task` child
//! launched through sudo (`sudo`).
use crate::context::BecomeMethod;
use crate::error::{Error, ErrorKind, Result};
use crate::task::{Task, TaskExecResult};

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output, Stdio, exit};

use ipc_channel::ipc::{self, IpcSender};
use minijinja::Value;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, Uid, User, fchown, fork, setgid, setuid};
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;
use tempfile::NamedTempFile;

/// Internal task serialization for sudo become method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTaskData {
    pub original_path: Option<String>,
    pub args: Option<Vec<String>>,
    pub vars: Value,
    pub task: YamlValue,
}

pub const RASH_INTERNAL_TASK_ENV: &str = "RASH_INTERNAL_TASK_FILE";
pub const RASH_INTERNAL_RESULT_ENV: &str = "RASH_INTERNAL_RESULT_FILE";
pub const RASH_INTERNAL_OUTPUT_ENV: &str = "RASH_INTERNAL_OUTPUT";
pub const RASH_INTERNAL_TASK_FLAG: &str = "RASH_INTERNAL";

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

impl Task<'_> {
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
        if self.become_method == BecomeMethod::Sudo {
            return self.exec_module_via_sudo(rendered_params, vars);
        }
        let user = self.resolve_become_user()?;
        if user.uid == Uid::current() {
            return self.execute_module_with_environment(rendered_params, vars);
        }
        if self.transfers_pid(rendered_params) {
            // The process is replaced on success: switching users in place is fine.
            return self.exec_module_rendered_with_user(rendered_params, vars, user);
        }
        self.exec_module_in_forked_child(rendered_params, vars, user)
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

    fn exec_module_rendered_with_user(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
        user: User,
    ) -> Result<TaskExecResult> {
        setgid(user.gid).map_err(|_| {
            Error::new(
                ErrorKind::Other,
                format!("gid cannot be changed to {}", user.gid),
            )
        })?;
        setuid(user.uid).map_err(|_| {
            Error::new(
                ErrorKind::Other,
                format!("uid cannot be changed to {}", user.uid),
            )
        })?;
        self.execute_module_with_environment(rendered_params, vars)
    }

    fn exec_module_in_forked_child(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
        user: User,
    ) -> Result<TaskExecResult> {
        let (tx, rx) = ipc::channel::<String>().map_err(|e| Error::new(ErrorKind::Other, e))?;
        // SAFETY: the child only runs the module and then always exits (see
        // `run_become_child`): it never returns into the parent's task loop.
        match unsafe { fork() }? {
            ForkResult::Child => self.run_become_child(&tx, rendered_params, vars, user),
            ForkResult::Parent { child } => {
                drop(tx);
                // Receive before reaping: a large result could block the child on send.
                let received = rx.recv();
                match waitpid(child, None)? {
                    WaitStatus::Exited(_, 0) => {}
                    status => {
                        return Err(Error::new(
                            ErrorKind::SubprocessFail,
                            format!("become child ended with status {status:?}"),
                        ));
                    }
                }
                let json = received.map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("become child sent no result: {e:?}"),
                    )
                })?;
                BecomeOutcome::from_json(&json)
            }
        }
    }

    /// Body of the forked become child: whatever happens, report it and terminate.
    fn run_become_child(
        &self,
        tx: &IpcSender<String>,
        rendered_params: &YamlValue,
        vars: &Value,
        user: User,
    ) -> ! {
        let outcome =
            BecomeOutcome::from(self.exec_module_rendered_with_user(rendered_params, vars, user));
        let sent = serde_json::to_string(&outcome)
            .map_err(|e| e.to_string())
            .and_then(|json| tx.send(json).map_err(|e| e.to_string()));
        match sent {
            Ok(()) => exit(0),
            Err(e) => {
                error!("become child failed to send result: {e}");
                exit(1)
            }
        }
    }

    fn internal_sudo_task(&self, rendered_params: &YamlValue) -> YamlValue {
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
        // The child must serialize semantic failure rather than terminating before the parent can
        // run rescue/always or apply the caller's ignore_errors policy.
        mapping.insert(key("ignore_errors"), YamlValue::Bool(true));
        YamlValue::Mapping(mapping)
    }

    fn internal_task_data(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
    ) -> Result<InternalTaskData> {
        Ok(InternalTaskData {
            original_path: vars
                .get_attr("rash")
                .ok()
                .and_then(|rash| rash.get_attr("path").ok())
                .and_then(|path| path.as_str().map(String::from)),
            args: None,
            vars: self.extend_vars(vars.clone())?,
            task: self.internal_sudo_task(rendered_params),
        })
    }

    /// Create a temporary file only the current user and the become user can access. Task
    /// files hold rendered params and vars, which may include secrets.
    fn sudo_private_file(&self, prefix: &str) -> Result<NamedTempFile> {
        // Created with O_EXCL, a random name and mode 0600; removed on drop.
        let file = tempfile::Builder::new()
            .prefix(prefix)
            .tempfile()
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
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

    fn sudo_command(&self, task_file: &Path, result_file: &Path) -> Result<StdCommand> {
        let rash_path = env::current_exe().map_err(|e| Error::new(ErrorKind::Other, e))?;
        let mut command = StdCommand::new(&self.become_exe);
        command.arg("-H").arg("-E").arg("-u").arg(&self.become_user);
        if self.become_password.is_some() {
            command.arg("-S");
        }
        command
            .arg("--")
            .arg(&rash_path)
            .arg("--internal-task")
            .arg(task_file)
            .env(RASH_INTERNAL_RESULT_ENV, result_file)
            .env(RASH_INTERNAL_TASK_FLAG, "1")
            .stdout(Stdio::inherit());
        Ok(command)
    }

    fn run_sudo(&self, mut command: StdCommand) -> Result<Output> {
        let Some(password) = &self.become_password else {
            let status = command
                .stderr(Stdio::inherit())
                .status()
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
            return Ok(Output {
                status,
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(format!("{password}\n").as_bytes())
                .map_err(|e| Error::new(ErrorKind::Other, e))?;
        }
        child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))
    }

    fn exec_module_via_sudo(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
    ) -> Result<TaskExecResult> {
        let mut task_file = self.sudo_private_file("rash_task_")?;
        let result_file = self.sudo_private_file("rash_result_")?;
        let task_content = serde_yaml::to_string(&self.internal_task_data(rendered_params, vars)?)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        task_file
            .write_all(task_content.as_bytes())
            .and_then(|()| task_file.flush())
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;

        let output = self.run_sudo(self.sudo_command(task_file.path(), result_file.path())?)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "{} failed with exit code {}: {}",
                    self.become_exe,
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        let result_content = fs::read_to_string(result_file.path()).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("Failed to read sudo result file: {e}"),
            )
        })?;
        BecomeOutcome::from_json(&result_content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GlobalParams;

    use std::os::unix::fs::PermissionsExt;

    fn task_with<'a>(yaml: &str, global_params: &'a GlobalParams<'a>) -> Task<'a> {
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
    fn sudo_private_file_is_owner_only() {
        let global_params = GlobalParams::default();
        let task = task_with("debug: {msg: hi}\nbecome_user: root", &global_params);
        let file = task.sudo_private_file("rash_test_").unwrap();
        let mode = file.as_file().metadata().unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn sudo_private_file_refuses_other_user_when_not_root() {
        if Uid::effective().is_root() {
            return;
        }
        let global_params = GlobalParams::default();
        let task = task_with("debug: {msg: hi}\nbecome_user: nobody", &global_params);
        assert!(task.sudo_private_file("rash_test_").is_err());
    }
}
