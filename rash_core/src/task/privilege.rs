//! Privilege escalation (`become`) for tasks.
//!
//! A module runs as another user in a `rash --internal-task` child process, started either
//! directly (`syscall`: the child switches user itself before running the task) or through
//! sudo (`sudo`). Both methods share one protocol: the task goes to the child in a private
//! temporary file, its outcome comes back in another one, and the child is supervised like
//! any foreground process, so signals are forwarded to it and `always` sections still run.
//! The child is a fresh process image: unlike a bare `fork`, it cannot inherit locks held
//! by other threads of the parent.
//!
//! The temporary directory may be writable by other users (e.g. a user's `TMPDIR` kept by
//! `sudo -E`), so the files are only trusted through what the parent controls: the child
//! gets the user to switch to and the owner of the files on its command line, refuses any
//! file that is not a private regular file of that owner, and the parent reads the outcome
//! through the descriptor it created instead of reopening the path.
use crate::context::{BecomeMethod, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::logger::suppress_logs;
use crate::process::{
    FAILED_REPLACEMENT_STATUS, OutputMode, ProcessResult, ProcessSpec, ProcessUser,
    exit_after_failed_replacement,
};
use crate::task::{Task, TaskExecResult};

use std::env;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
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
    /// `uid:gid:name`, the form a become child gets on its command line.
    fn to_arg(&self) -> String {
        format!("{}:{}:{}", self.uid, self.gid, self.name)
    }

    fn from_arg(arg: &str) -> Option<Self> {
        let mut parts = arg.splitn(3, ':');
        let uid = parts.next()?.parse().ok()?;
        let gid = parts.next()?.parse().ok()?;
        let name = parts.next().filter(|name| !name.is_empty())?;
        Some(Self {
            name: name.to_owned(),
            uid,
            gid,
        })
    }

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

    /// This user as a child process runs it, with its supplementary groups resolved now:
    /// a child cannot look them up between `fork` and `exec`.
    pub fn process_user(&self) -> Result<ProcessUser> {
        let groups = supplementary_groups(&self.name, Gid::from_raw(self.gid)).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!(
                    "cannot become user {}: groups cannot be read: {e}",
                    self.name
                ),
            )
        })?;
        Ok(ProcessUser {
            uid: self.uid,
            gid: self.gid,
            groups,
        })
    }
}

#[cfg(not(any(target_vendor = "apple", target_os = "redox", target_os = "haiku")))]
fn supplementary_groups(name: &str, gid: Gid) -> Result<Vec<u32>> {
    let name = std::ffi::CString::new(name).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    let groups = nix::unistd::getgrouplist(&name, gid)?;
    Ok(groups.into_iter().map(Gid::as_raw).collect())
}

/// nix has no `getgrouplist(3)` binding on Apple targets, whose gids are `int`s.
#[cfg(target_vendor = "apple")]
fn supplementary_groups(name: &str, gid: Gid) -> Result<Vec<u32>> {
    let name = std::ffi::CString::new(name).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    // Wrapping is intended: gids above i32::MAX, like nobody's (-2), are negative ints.
    let gid = gid.as_raw() as libc::c_int;
    let mut capacity: libc::c_int = 32;
    loop {
        let mut groups: Vec<libc::c_int> = vec![0; capacity as usize];
        let mut count = capacity;
        // SAFETY: `groups` holds `count` ints and `name` is NUL-terminated; both outlive
        // the call, which writes at most `count` entries and the final count to `count`.
        let result =
            unsafe { libc::getgrouplist(name.as_ptr(), gid, groups.as_mut_ptr(), &mut count) };
        if result != -1 {
            groups.truncate(count.max(0) as usize);
            return Ok(groups.into_iter().map(|gid| gid as u32).collect());
        }
        // The list did not fit: retry with a larger buffer, up to a sane bound.
        if capacity >= 1 << 16 {
            return Err(Error::new(
                ErrorKind::Other,
                "too many supplementary groups",
            ));
        }
        capacity *= 2;
    }
}

/// No `getgrouplist(3)` here: only the primary group, matching `set_supplementary_groups`.
#[cfg(any(target_os = "redox", target_os = "haiku"))]
fn supplementary_groups(_name: &str, gid: Gid) -> Result<Vec<u32>> {
    Ok(vec![gid.as_raw()])
}

#[cfg(not(any(target_vendor = "apple", target_os = "redox", target_os = "haiku")))]
fn set_supplementary_groups(name: &str, gid: Gid) -> Result<()> {
    let name = std::ffi::CString::new(name).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    nix::unistd::initgroups(&name, gid)?;
    Ok(())
}

/// nix has no `initgroups(3)` binding on Apple targets, whose group argument is an `int`.
#[cfg(target_vendor = "apple")]
fn set_supplementary_groups(name: &str, gid: Gid) -> Result<()> {
    let name = std::ffi::CString::new(name).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    // Wrapping is intended: gids above i32::MAX, like nobody's (-2), are negative ints.
    let gid = gid.as_raw() as libc::c_int;
    // SAFETY: `name` is a NUL-terminated string that outlives the call.
    if unsafe { libc::initgroups(name.as_ptr(), gid) } != 0 {
        return Err(Error::new(
            ErrorKind::Other,
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

/// No `initgroups(3)` here: keep only the primary group, never the caller's groups.
#[cfg(any(target_os = "redox", target_os = "haiku"))]
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

/// How a become child runs as the become user. Given on its command line by the parent and
/// never read from the task file, which lives in a possibly shared directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildBecome {
    /// Started as the become user by sudo: nothing to switch.
    Sudo,
    /// Started as Rash's user: switch to this user before running the task.
    Switch(BecomeUser),
}

impl ChildBecome {
    const SUDO_ARG: &str = "sudo";

    fn to_arg(&self) -> String {
        match self {
            Self::Sudo => Self::SUDO_ARG.to_owned(),
            Self::Switch(user) => user.to_arg(),
        }
    }

    fn from_arg(arg: &str) -> Result<Self> {
        if arg == Self::SUDO_ARG {
            return Ok(Self::Sudo);
        }
        BecomeUser::from_arg(arg).map(Self::Switch).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("invalid become user {arg:?}: expected `sudo` or `uid:gid:name`"),
            )
        })
    }
}

/// Upper bound of a task or result file, far above any real task or registered output.
const MAX_EXCHANGE_FILE_SIZE: u64 = 256 * 1024 * 1024;

/// Open a task or result file created by the parent Rash, refusing anything else: it lives
/// in a temporary directory other users may write to, where the path could be replaced by
/// a symlink, a hard link, a FIFO or a file of another user.
fn open_exchange_file(path: &Path, owner: u32, options: &mut OpenOptions) -> Result<File> {
    let file = options
        // Never follow a symlink nor block on a FIFO.
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("cannot open {}: {e}", path.display()),
            )
        })?;
    let metadata = file
        .metadata()
        .map_err(|e| Error::new(ErrorKind::IOError, e))?;
    check_exchange_file(&metadata, owner).map_err(|problem| {
        Error::new(
            ErrorKind::InvalidData,
            format!("refusing {}: {problem}", path.display()),
        )
    })?;
    Ok(file)
}

/// Why `metadata` is not a private regular file of `owner` with a single link, if it is not.
fn check_exchange_file(metadata: &Metadata, owner: u32) -> std::result::Result<(), String> {
    if !metadata.file_type().is_file() {
        return Err("not a regular file".to_owned());
    }
    if metadata.uid() != owner {
        return Err(format!(
            "owned by uid {} instead of {owner}",
            metadata.uid()
        ));
    }
    if metadata.mode() & 0o077 != 0 {
        return Err(format!(
            "accessible by other users (mode {:o})",
            metadata.mode() & 0o7777
        ));
    }
    if metadata.nlink() != 1 {
        return Err(format!("{} hard links instead of 1", metadata.nlink()));
    }
    Ok(())
}

/// Read a whole task or result file, up to [`MAX_EXCHANGE_FILE_SIZE`].
fn read_exchange_file(file: &mut File, what: &str) -> Result<String> {
    let mut content = String::new();
    file.take(MAX_EXCHANGE_FILE_SIZE + 1)
        .read_to_string(&mut content)
        .map_err(|e| Error::new(ErrorKind::IOError, format!("Failed to read {what}: {e}")))?;
    if content.len() as u64 > MAX_EXCHANGE_FILE_SIZE {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("{what} exceeds {MAX_EXCHANGE_FILE_SIZE} bytes"),
        ));
    }
    Ok(content)
}

pub const RASH_INTERNAL_TASK_ENV: &str = "RASH_INTERNAL_TASK_FILE";
pub const RASH_INTERNAL_RESULT_ENV: &str = "RASH_INTERNAL_RESULT_FILE";
pub const RASH_INTERNAL_OUTPUT_ENV: &str = "RASH_INTERNAL_OUTPUT";
pub const RASH_INTERNAL_TASK_FLAG: &str = "RASH_INTERNAL";
/// Fallback verbosity of the `rash` binary when no `-v` flag is given.
pub const RASH_LOG_LEVEL_ENV: &str = "RASH_LOG_LEVEL";

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
///
/// `file_owner` is the owner of the task and result files and `child_become` how the child
/// runs as the become user, both given by the parent: without them the child refuses to
/// run rather than guess, which could run the task as root.
pub fn execute_internal_task(
    task_path: &Path,
    file_owner: Option<u32>,
    child_become: Option<&str>,
) -> Result<()> {
    let missing = |flag: &str| {
        Error::new(
            ErrorKind::InvalidData,
            format!("internal task requires {flag}"),
        )
    };
    let owner = file_owner.ok_or_else(|| missing("--internal-task-owner"))?;
    let child_become =
        ChildBecome::from_arg(child_become.ok_or_else(|| missing("--internal-become"))?)?;
    let data = read_internal_task(task_path, owner)?;
    // Opened before switching user: the parent created it private to its own user.
    let mut result_file = open_result_file(owner)?;
    let outcome = BecomeOutcome::from(run_internal_task(data, &child_become));
    let json = serde_json::to_string(&outcome).map_err(|e| Error::new(ErrorKind::Other, e))?;
    result_file.write_all(json.as_bytes()).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to write result file: {e}"),
        )
    })
}

fn read_internal_task(task_path: &Path, owner: u32) -> Result<InternalTaskData> {
    let mut file = open_exchange_file(task_path, owner, OpenOptions::new().read(true))?;
    let content = read_exchange_file(&mut file, "internal task file")?;
    serde_yaml::from_str(&content).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse internal task data: {e}"),
        )
    })
}

fn open_result_file(owner: u32) -> Result<File> {
    let path = get_internal_result_path()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "No result file path specified"))?;
    // Never create a result file.
    let mut options = OpenOptions::new();
    options.write(true);
    let file = open_exchange_file(&path, owner, &mut options)?;
    // Truncated once validated: never a file that failed validation.
    file.set_len(0)
        .map_err(|e| Error::new(ErrorKind::IOError, e))?;
    Ok(file)
}

/// Whether a task sent by the parent is a `no_log` one (see `Task::internal_task`).
fn is_no_log_internal_task(task: &YamlValue) -> bool {
    task.get("no_log").and_then(YamlValue::as_bool) == Some(true)
}

fn run_internal_task(data: InternalTaskData, child_become: &ChildBecome) -> Result<TaskExecResult> {
    if let ChildBecome::Switch(user) = child_become {
        user.switch()?;
    }
    // Suppressed before parsing: the task holds rendered params, secrets included.
    let _no_log_guard = is_no_log_internal_task(&data.task).then(suppress_logs);
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
                // The process is replaced on success: switching users in place is fine. But
                // a failed switch may be partial (groups and gid changed, uid not): Rash
                // must stop rather than report an error the script could ignore.
                if let Err(error) = BecomeUser::from(&user).switch() {
                    exit_after_failed_replacement(error, FAILED_REPLACEMENT_STATUS);
                }
                let result = self.execute_module_with_environment(rendered_params, vars);
                exit_after_failed_transfer(&user.name, result);
            }
        }
        self.exec_module_in_become_child(rendered_params, vars)
    }

    fn child_become(&self) -> Result<ChildBecome> {
        Ok(match self.become_method {
            BecomeMethod::Syscall => {
                ChildBecome::Switch(BecomeUser::from(&self.resolve_become_user()?))
            }
            BecomeMethod::Sudo => ChildBecome::Sudo,
        })
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
        Ok(InternalTaskData {
            vars: self.extend_vars(vars.clone())?,
            task: self.internal_task(rendered_params),
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

    /// Path of this Rash binary to start a become child with.
    fn rash_exe(&self) -> Result<String> {
        // Still this binary if it was replaced or removed meanwhile (e.g. by an upgrade).
        // Only when Rash executes the child itself: sudo would resolve it to sudo.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if self.become_method == BecomeMethod::Syscall {
            let proc_exe = Path::new("/proc/self/exe");
            if proc_exe.exists() {
                return path_arg(proc_exe);
            }
        }
        let rash = env::current_exe().map_err(|e| Error::new(ErrorKind::Other, e))?;
        path_arg(&rash)
    }

    fn become_child_spec(
        &self,
        task_file: &Path,
        result_file: &Path,
        file_owner: u32,
    ) -> Result<ProcessSpec> {
        let rash = self.rash_exe()?;
        let mut internal_args = vec![
            "--internal-task".to_owned(),
            path_arg(task_file)?,
            "--internal-task-owner".to_owned(),
            file_owner.to_string(),
            "--internal-become".to_owned(),
            self.child_become()?.to_arg(),
        ];
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
            // Only the flags above set the child's verbosity, never an inherited variable.
            (RASH_LOG_LEVEL_ENV.to_owned(), String::new()),
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
        let mut result_file = self.private_file("rash_result_")?;
        let task_content = serde_yaml::to_string(&self.internal_task_data(rendered_params, vars)?)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        task_file
            .write_all(task_content.as_bytes())
            .and_then(|()| task_file.flush())
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        let file_owner = task_file
            .as_file()
            .metadata()
            .map_err(|e| Error::new(ErrorKind::IOError, e))?
            .uid();

        let spec = self.become_child_spec(task_file.path(), result_file.path(), file_owner)?;
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
        // Through the descriptor created above: the path may have been replaced.
        let result_file = result_file.as_file_mut();
        result_file
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        BecomeOutcome::from_json(&read_exchange_file(result_file, "become result file")?)
    }
}

/// Reason and exit status when a `transfer_pid` task returned instead of replacing Rash:
/// its failure with [`FAILED_REPLACEMENT_STATUS`], or the status of an explicit
/// termination (exit, interrupt), which keeps its meaning.
fn replacement_failure(result: &Result<TaskExecResult>) -> (String, i32) {
    match result {
        Ok(result) => (
            result
                .get_error()
                .unwrap_or("process not replaced")
                .to_owned(),
            FAILED_REPLACEMENT_STATUS,
        ),
        Err(error) if error.is_termination() => (
            error.to_string(),
            error.raw_os_error().unwrap_or(FAILED_REPLACEMENT_STATUS),
        ),
        Err(error) => (error.to_string(), FAILED_REPLACEMENT_STATUS),
    }
}

/// A `transfer_pid` task returned after Rash switched to the become user in place, so the
/// process was not replaced: stop instead of running further tasks (or `always` sections)
/// as that user.
fn exit_after_failed_transfer(user: &str, result: Result<TaskExecResult>) -> ! {
    let (reason, code) = replacement_failure(&result);
    exit_after_failed_replacement(format!("after switching to user {user}: {reason}"), code)
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

    /// A private file of the current user in a fresh directory, and the directory.
    fn exchange_file() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task");
        std::fs::write(&path, "content").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        (dir, path)
    }

    fn open_for_read(path: &Path, owner: u32) -> Result<File> {
        open_exchange_file(path, owner, OpenOptions::new().read(true))
    }

    #[test]
    fn exchange_file_accepts_private_file_of_owner() {
        let (_dir, path) = exchange_file();
        let mut file = open_for_read(&path, Uid::effective().as_raw()).unwrap();
        assert_eq!(read_exchange_file(&mut file, "task").unwrap(), "content");
    }

    #[test]
    fn exchange_file_rejects_other_owner() {
        let (_dir, path) = exchange_file();
        let other = Uid::effective().as_raw().wrapping_add(1);
        let error = open_for_read(&path, other).unwrap_err();
        assert!(error.to_string().contains("owned by uid"), "{error}");
    }

    #[test]
    fn exchange_file_rejects_group_or_world_access() {
        let (_dir, path) = exchange_file();
        for mode in [0o640, 0o604, 0o660] {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            let error = open_for_read(&path, Uid::effective().as_raw()).unwrap_err();
            assert!(
                error.to_string().contains("accessible by other users"),
                "{error}"
            );
        }
    }

    #[test]
    fn exchange_file_rejects_symlink() {
        let (dir, path) = exchange_file();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let error = open_for_read(&link, Uid::effective().as_raw()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::IOError, "{error}");
    }

    #[test]
    fn exchange_file_rejects_hard_link() {
        let (dir, path) = exchange_file();
        std::fs::hard_link(&path, dir.path().join("link")).unwrap();
        let error = open_for_read(&path, Uid::effective().as_raw()).unwrap_err();
        assert!(error.to_string().contains("hard links"), "{error}");
    }

    #[test]
    fn exchange_file_rejects_fifo_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("fifo");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
        let error = open_for_read(&fifo, Uid::effective().as_raw()).unwrap_err();
        assert!(error.to_string().contains("not a regular file"), "{error}");
    }

    #[test]
    fn child_become_round_trips_through_its_argument() {
        let user = BecomeUser {
            name: "nobody".to_owned(),
            uid: 65534,
            gid: 4294967294,
        };
        for child_become in [ChildBecome::Sudo, ChildBecome::Switch(user)] {
            let parsed = ChildBecome::from_arg(&child_become.to_arg()).unwrap();
            assert_eq!(parsed, child_become);
        }
        for invalid in ["", "root", "0:0", "0:0:", "x:0:root", "0:-1:root"] {
            assert!(ChildBecome::from_arg(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn replacement_failure_maps_outcomes_to_exit_status() {
        let failed = TaskExecResult::failed(false, None, "no such program");
        assert_eq!(
            replacement_failure(&Ok(failed)),
            ("no such program".to_owned(), 1)
        );
        assert_eq!(
            replacement_failure(&Ok(TaskExecResult::new(true, None))),
            ("process not replaced".to_owned(), 1)
        );
        let (reason, code) = replacement_failure(&Err(Error::new(ErrorKind::Other, "boom")));
        assert!(reason.contains("boom"), "{reason}");
        assert_eq!(code, 1);
        assert_eq!(replacement_failure(&Err(Error::explicit_exit(7))).1, 7);
        assert_eq!(replacement_failure(&Err(Error::interrupted(15))).1, 143);
    }

    #[test]
    fn internal_task_carries_no_log_as_a_flag() {
        let global_params = GlobalParams::default();
        let params: YamlValue = serde_norway::from_str("argv: [echo, secret]").unwrap();
        let task = task_with("command: echo secret\nno_log: true", &global_params);
        assert!(is_no_log_internal_task(&task.internal_task(&params)));
        let task = task_with("command: echo secret", &global_params);
        assert!(!is_no_log_internal_task(&task.internal_task(&params)));
        // Only the exact flag the parent sets, never something templated or truthy.
        let yaml: YamlValue = serde_norway::from_str("command: x\nno_log: 'true'").unwrap();
        assert!(!is_no_log_internal_task(&yaml));
    }

    #[test]
    fn become_child_spec_resets_inherited_log_level() {
        let global_params = GlobalParams::default();
        let task = task_with("command: id\nbecome: true", &global_params);
        let spec = task
            .become_child_spec(Path::new("/tmp/task"), Path::new("/tmp/result"), 0)
            .unwrap();
        assert!(
            spec.env
                .contains(&(RASH_LOG_LEVEL_ENV.to_owned(), String::new())),
            "{:?}",
            spec.env
        );
    }

    #[test]
    fn process_user_resolves_supplementary_groups() {
        let current = User::from_uid(Uid::current()).unwrap().unwrap();
        let user = BecomeUser::from(&current).process_user().unwrap();
        assert_eq!(user.uid, current.uid.as_raw());
        assert_eq!(user.gid, current.gid.as_raw());
        assert!(user.groups.contains(&current.gid.as_raw()));
    }

    #[test]
    fn internal_task_refuses_to_run_without_owner_or_become_user() {
        let (_dir, path) = exchange_file();
        let owner = Some(Uid::effective().as_raw());
        for (owner, child_become) in [(owner, None), (None, Some("sudo"))] {
            let error = execute_internal_task(&path, owner, child_become).unwrap_err();
            assert!(
                error.to_string().contains("internal task requires"),
                "{error}"
            );
        }
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
