use crate::error::{Error, ErrorKind, Result};
use crate::modules::ModuleResult;
use crate::signal::{self, ForegroundGuard};

use std::io::{self, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;

/// How long to keep draining output after an interrupt before abandoning pipes that
/// background grandchildren still hold open.
const INTERRUPT_OUTPUT_GRACE: Duration = Duration::from_millis(200);
const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Capture,
    Inherit,
    Null,
    Tee,
}

impl OutputMode {
    fn is_piped(self) -> bool {
        matches!(self, Self::Capture | Self::Tee)
    }

    fn stdio(self) -> Stdio {
        match self {
            Self::Capture | Self::Tee => Stdio::piped(),
            Self::Inherit => Stdio::inherit(),
            Self::Null => Stdio::null(),
        }
    }
}

/// Exit status of Rash when a `transfer_pid` task leaves it running instead of replacing
/// it, like a non-interactive `sh` whose `exec` fails.
pub const FAILED_REPLACEMENT_STATUS: i32 = 1;

/// Terminate Rash after a `transfer_pid` task did not replace it. The process is no longer
/// fit to run tasks: a failed `exec` has already applied the chdir, the stdio redirections
/// and the signal resets meant for the program, and with `become` the user was switched in
/// place. Nothing after this runs: no `rescue` or `always` section, nor `ignore_errors`.
pub fn exit_after_failed_replacement(reason: impl std::fmt::Display, code: i32) -> ! {
    error!("transfer_pid failed, exiting: {reason}");
    std::process::exit(code)
}

/// Credentials a child runs with instead of Rash's, resolved before spawning it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessUser {
    pub uid: libc::uid_t,
    pub gid: libc::gid_t,
    /// The whole supplementary group list, as `initgroups(3)` would set it.
    pub groups: Vec<libc::gid_t>,
}

impl ProcessUser {
    /// Switch the current process to this user. Runs between `fork` and `exec`, so it only
    /// makes async-signal-safe syscalls and never allocates: another thread of the parent
    /// may hold the allocator lock at the time of the fork.
    fn apply(&self) -> io::Result<()> {
        // SAFETY: setgroups(2) reads `groups.len()` gids from a live buffer.
        if unsafe { libc::setgroups(self.groups.len() as _, self.groups.as_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: setgid(2) and setuid(2) have no preconditions.
        if unsafe { libc::setgid(self.gid) } != 0 || unsafe { libc::setuid(self.uid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub chdir: Option<String>,
    pub stdin: Option<String>,
    pub stdout: OutputMode,
    pub stderr: OutputMode,
    pub env: Vec<(String, String)>,
    /// Run the child as this user instead of Rash's.
    pub user: Option<ProcessUser>,
    /// Run the child as a background job in its own process group, so its whole tree can
    /// be killed, and without `stdin` data on an empty stdin. Only async jobs need it:
    /// synchronous children stay in Rash's process group, so they behave as a foreground
    /// job on the controlling terminal (they can read it and receive Ctrl-C).
    pub process_group: bool,
}

impl ProcessSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            chdir: None,
            stdin: None,
            stdout: OutputMode::Capture,
            stderr: OutputMode::Capture,
            env: Vec::new(),
            user: None,
            process_group: false,
        }
    }

    pub fn shell(command: impl Into<String>, executable: impl Into<String>) -> Self {
        let mut spec = Self::new(executable);
        spec.args = vec!["-c".to_owned(), command.into()];
        spec
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        if let Some(chdir) = &self.chdir {
            command.current_dir(Path::new(chdir));
        }
        for (key, value) in &self.env {
            command.env(key, value);
        }
        command.stdin(match (&self.stdin, self.process_group) {
            (Some(_), _) => Stdio::piped(),
            // A background job reading the terminal would be stopped by SIGTTIN, and it
            // would compete with the following tasks for Rash's input.
            (None, true) => Stdio::null(),
            (None, false) => Stdio::inherit(),
        });
        command.stdout(self.stdout.stdio());
        command.stderr(self.stderr.stdio());
        if self.process_group {
            command.process_group(0);
        }
        if let Some(user) = &self.user {
            let user = user.clone();
            // SAFETY: `apply` only makes async-signal-safe syscalls on data owned by the
            // closure, which is allocated here, before the fork.
            unsafe { command.pre_exec(move || user.apply()) };
        }
        command
    }

    pub fn spawn_managed(&self) -> Result<SpawnedProcess> {
        let mut command = self.command();
        trace!("spawn process: {:?} {:?}", self.program, self.args);
        let child = command.spawn().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute '{}': {e}", self.program),
            )
        })?;
        let mut process = SpawnedProcess {
            child,
            streams: Streams::default(),
            process_group: self.process_group,
        };
        if let Err(error) = process.start_streams(self) {
            process.abort();
            return Err(error);
        }
        Ok(process)
    }

    /// Run the child to completion as Rash's foreground job.
    ///
    /// Returns [`ErrorKind::Interrupted`] if Rash received a termination signal meanwhile.
    pub fn run(&self) -> Result<ProcessResult> {
        // Set up before spawning: a signal arriving during the spawn is recorded and
        // forwarded on attach instead of terminating Rash with an untracked child.
        let guard = ForegroundGuard::new();
        let mut process = self.spawn_managed()?;
        guard.attach(process.id());
        let status = match process.wait_foreground(&guard) {
            Ok(status) => status,
            Err(error) => {
                process.abort();
                return Err(error);
            }
        };
        if let Some(interrupt) = guard.take_interrupt(status.signal().is_some()) {
            let _ = process.finish_within(status, INTERRUPT_OUTPUT_GRACE);
            return Err(interrupt);
        }
        let result = process.finish(status)?;
        // A signal received while draining output: the child is gone, so always honor it.
        match guard.take_interrupt(true) {
            Some(interrupt) => Err(interrupt),
            None => Ok(result),
        }
    }

    /// Spec used to replace the current process: after `exec` no Rash code remains to
    /// feed stdin or drain pipes, so captured streams are inherited instead of piped.
    fn replacement_spec(&self) -> Result<Self> {
        if self.stdin.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "stdin cannot be combined with transfer_pid",
            ));
        }
        let inherit_piped = |mode: OutputMode| {
            if mode.is_piped() {
                OutputMode::Inherit
            } else {
                mode
            }
        };
        let mut spec = self.clone();
        spec.process_group = false;
        spec.stdout = inherit_piped(self.stdout);
        spec.stderr = inherit_piped(self.stderr);
        Ok(spec)
    }

    /// Replace the Rash process with this one, like `exec` in a shell. Returns only when
    /// the spec cannot replace a process, before anything was done. Once `exec` is attempted
    /// and fails, Rash exits with [`FAILED_REPLACEMENT_STATUS`]: see
    /// [`exit_after_failed_replacement`].
    pub fn replace(&self) -> Error {
        let spec = match self.replacement_spec() {
            Ok(spec) => spec,
            Err(e) => return e,
        };
        let error = spec.command().exec();
        exit_after_failed_replacement(
            format!("Failed to execute '{}': {error}", self.program),
            FAILED_REPLACEMENT_STATUS,
        )
    }
}

/// What a module running a single process does with its params, decided without running
/// anything, so sync and async executions share the same semantics.
#[derive(Debug)]
pub enum ProcessPlan {
    /// Nothing to run: check mode, or a `creates`/`removes` condition already met.
    Done(ModuleResult),
    /// Run the process to completion.
    Run(ProcessSpec),
    /// Replace the Rash process with it (`transfer_pid`); Rash exits if that fails.
    Replace(ProcessSpec),
}

impl ProcessPlan {
    /// Execute the plan synchronously, as the module itself does.
    pub fn execute(self) -> Result<ModuleResult> {
        match self {
            Self::Done(result) => Ok(result),
            Self::Run(spec) => {
                let result = spec.run()?;
                trace!("exec - process result: {result:?}");
                result.into_module_result()
            }
            Self::Replace(spec) => Err(spec.replace()),
        }
    }
}

type Buffer = Arc<Mutex<Vec<u8>>>;

/// Background threads feeding stdin and draining stdout/stderr of a child.
#[derive(Default)]
struct Streams {
    done: Option<Receiver<io::Result<()>>>,
    running: usize,
    stdout: Option<Buffer>,
    stderr: Option<Buffer>,
}

impl Streams {
    /// Wait for the stream threads to finish, until `deadline` (if any) or until Rash
    /// receives a termination signal. Streams still running are abandoned: background
    /// grandchildren may hold the pipes open indefinitely.
    fn wait(&mut self, deadline: Option<Instant>) -> Result<()> {
        let Some(done) = &self.done else {
            return Ok(());
        };
        while self.running > 0 && !signal::interrupt_pending() {
            let timeout = match deadline {
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    left.min(STREAM_POLL_INTERVAL)
                }
                None => STREAM_POLL_INTERVAL,
            };
            match done.recv_timeout(timeout) {
                Ok(result) => {
                    self.running -= 1;
                    result.map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Error::new(
                        ErrorKind::SubprocessFail,
                        "process stream thread panicked",
                    ));
                }
            }
        }
        if self.running > 0 {
            debug!(
                "{} process stream(s) still open, likely held by background processes",
                self.running
            );
        }
        Ok(())
    }
}

pub struct SpawnedProcess {
    child: Child,
    streams: Streams,
    process_group: bool,
}

impl std::fmt::Debug for SpawnedProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnedProcess")
            .field("pid", &self.child.id())
            .field("process_group", &self.process_group)
            .finish_non_exhaustive()
    }
}

impl SpawnedProcess {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub fn wait(&mut self) -> Result<ExitStatus> {
        self.child
            .wait()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))
    }

    fn wait_foreground(&mut self, guard: &ForegroundGuard) -> Result<ExitStatus> {
        if let Err(error) = signal::wait_exited(self.id()) {
            debug!("waitid failed, reaping directly: {error}");
        }
        guard.detach();
        self.wait()
    }

    pub fn kill_tree(&mut self) -> std::io::Result<()> {
        if self.process_group {
            let pgid = self.child.id() as i32;
            // SAFETY: ProcessSpec created this child with process_group(0).
            let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };
            if result == 0 {
                return Ok(());
            }
        }
        self.child.kill()
    }

    /// Kill and reap the child after a setup failure.
    fn abort(&mut self) {
        let _ = self.kill_tree();
        let _ = self.child.wait();
    }

    fn start_streams(&mut self, spec: &ProcessSpec) -> Result<()> {
        let (done_tx, done_rx) = mpsc::channel();
        // Drain piped output before feeding stdin. Otherwise a child that writes output
        // while consuming a large stdin (for example `cat`) deadlocks once pipes fill.
        if spec.stdout.is_piped()
            && let Some(reader) = self.child.stdout.take()
        {
            let tee = (spec.stdout == OutputMode::Tee).then_some(TeeTarget::Stdout);
            self.streams.stdout = Some(spawn_reader(reader, tee, done_tx.clone())?);
            self.streams.running += 1;
        }
        if spec.stderr.is_piped()
            && let Some(reader) = self.child.stderr.take()
        {
            let tee = (spec.stderr == OutputMode::Tee).then_some(TeeTarget::Stderr);
            self.streams.stderr = Some(spawn_reader(reader, tee, done_tx.clone())?);
            self.streams.running += 1;
        }
        if let Some(data) = &spec.stdin
            && let Some(handle) = self.child.stdin.take()
        {
            spawn_writer(handle, data.clone(), done_tx)?;
            self.streams.running += 1;
        }
        self.streams.done = Some(done_rx);
        Ok(())
    }

    /// Collect captured output after the child exited, waiting until every stream hits
    /// EOF. Stops early, keeping what was read, if Rash receives a termination signal.
    pub fn finish(self, status: ExitStatus) -> Result<ProcessResult> {
        self.collect(status, None)
    }

    /// Like [`finish`](Self::finish), but gives up on streams still open after `grace`
    /// (held by background grandchildren), returning the output read so far.
    pub fn finish_within(self, status: ExitStatus, grace: Duration) -> Result<ProcessResult> {
        self.collect(status, Some(Instant::now() + grace))
    }

    fn collect(mut self, status: ExitStatus, deadline: Option<Instant>) -> Result<ProcessResult> {
        drop(self.child.stdin.take());
        self.streams.wait(deadline)?;
        Ok(ProcessResult {
            status,
            stdout: take_output(self.streams.stdout.take()),
            stderr: take_output(self.streams.stderr.take()),
        })
    }
}

#[derive(Clone, Copy)]
enum TeeTarget {
    Stdout,
    Stderr,
}

impl TeeTarget {
    fn write(self, data: &[u8]) -> io::Result<()> {
        match self {
            Self::Stdout => {
                let mut out = io::stdout().lock();
                out.write_all(data)?;
                out.flush()
            }
            Self::Stderr => {
                let mut out = io::stderr().lock();
                out.write_all(data)?;
                out.flush()
            }
        }
    }
}

fn spawn_reader<R>(
    reader: R,
    tee: Option<TeeTarget>,
    done: Sender<io::Result<()>>,
) -> Result<Buffer>
where
    R: Read + Send + 'static,
{
    let buffer = Buffer::default();
    let sink = Arc::clone(&buffer);
    thread::Builder::new()
        .name("rash-output".to_owned())
        .spawn(move || {
            let _ = done.send(drain(reader, &sink, tee));
        })
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    Ok(buffer)
}

fn drain<R: Read>(
    mut reader: R,
    sink: &Mutex<Vec<u8>>,
    mut tee: Option<TeeTarget>,
) -> io::Result<()> {
    let mut chunk = [0_u8; 8192];
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(read) => read,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(&chunk[..read]);
        if let Some(target) = tee
            && target.write(&chunk[..read]).is_err()
        {
            // Keep draining so the child never blocks on a full pipe.
            tee = None;
        }
    }
}

fn spawn_writer(mut handle: ChildStdin, data: String, done: Sender<io::Result<()>>) -> Result<()> {
    thread::Builder::new()
        .name("rash-stdin".to_owned())
        .spawn(move || {
            let result = match handle.write_all(data.as_bytes()) {
                // The child exited or closed stdin without reading all of it (`head -c1`,
                // `grep -q`): not an error, its exit status tells the outcome.
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
                result => result,
            };
            // Close stdin so the child sees EOF.
            drop(handle);
            let _ = done.send(result);
        })
        .map(drop)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))
}

fn take_output(buffer: Option<Buffer>) -> Option<String> {
    let bytes = std::mem::take(&mut *buffer?.lock().unwrap_or_else(PoisonError::into_inner));
    (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Debug)]
pub struct ProcessResult {
    pub status: ExitStatus,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

impl ProcessResult {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Module result of a finished process: always changed, failed on a non-zero status.
    pub fn into_module_result(self) -> Result<ModuleResult> {
        let failed = !self.success();
        let extra = serde_norway::value::to_value(json!({
            "rc": self.rc(),
            "stderr": self.stderr.unwrap_or_default(),
            "failed": failed,
        }))?;
        Ok(ModuleResult::new(true, Some(extra), self.stdout))
    }

    pub fn rc(&self) -> i32 {
        if let Some(code) = self.status.code() {
            return code;
        }
        if let Some(signal) = self.status.signal() {
            return 128 + signal;
        }
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(command: &str) -> ProcessSpec {
        ProcessSpec::shell(command, "/bin/sh")
    }

    #[test]
    fn captures_stdout_and_status() {
        let result = shell("printf hello").run().unwrap();
        assert!(result.success());
        assert_eq!(result.rc(), 0);
        assert_eq!(result.stdout.as_deref(), Some("hello"));
    }

    #[test]
    fn replacement_inherits_piped_output() {
        let mut spec = ProcessSpec::new("true");
        spec.stderr = OutputMode::Null;
        spec.process_group = true;
        let replacement = spec.replacement_spec().unwrap();
        assert_eq!(replacement.stdout, OutputMode::Inherit);
        assert_eq!(replacement.stderr, OutputMode::Null);
        assert!(!replacement.process_group);
    }

    /// A spec that cannot replace a process is an ordinary error, reported before `exec`
    /// is attempted (a failed `exec` would exit the test process instead).
    #[test]
    fn replacement_rejects_stdin() {
        let mut spec = ProcessSpec::new("cat");
        spec.stdin = Some("data".into());
        assert!(spec.replacement_spec().is_err());
        assert!(spec.replace().to_string().contains("stdin"));
    }

    /// Root-only (setgroups(2) needs CAP_SETGID, even for the caller's own list): the
    /// child gets exactly the given uid, gid and supplementary groups.
    #[test]
    fn test_as_root_process_user_applies_groups_gid_and_uid() {
        if !nix::unistd::Uid::effective().is_root() {
            eprintln!(
                "test_as_root_process_user_applies_groups_gid_and_uid: skipped: requires root"
            );
            return;
        }
        let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
        let (uid, gid) = (nobody.uid.as_raw(), nobody.gid.as_raw());
        // An unrelated extra group, to tell the list apart from initgroups(3)'s.
        let extra = nix::unistd::Group::from_name("daemon")
            .unwrap()
            .map_or(1, |group| group.gid.as_raw());
        let mut spec = shell("id -u; id -g; id -G");
        spec.user = Some(ProcessUser {
            uid,
            gid,
            groups: vec![gid, extra],
        });
        let result = spec.run().unwrap();
        assert!(result.success(), "{result:?}");
        let output = result.stdout.unwrap();
        let mut lines = output.lines();
        assert_eq!(lines.next().unwrap(), uid.to_string(), "{output}");
        assert_eq!(lines.next().unwrap(), gid.to_string(), "{output}");
        let mut reported: Vec<u32> = lines
            .next()
            .unwrap()
            .split_whitespace()
            .map(|gid| gid.parse().unwrap())
            .collect();
        reported.sort_unstable();
        reported.dedup();
        let mut expected = vec![gid, extra];
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(reported, expected, "{output}");
    }

    #[test]
    fn process_user_failure_is_a_spawn_error() {
        if nix::unistd::Uid::effective().is_root() {
            return;
        }
        let mut spec = shell("id -u");
        spec.user = Some(ProcessUser {
            uid: 0,
            gid: 0,
            groups: vec![0],
        });
        let error = spec.run().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::SubprocessFail);
        assert!(error.to_string().contains("Failed to execute"), "{error}");
    }

    #[test]
    fn carries_environment_without_mutating_parent() {
        let mut spec = shell("printf %s \"$RASH_PROCESS_TEST\"");
        spec.env.push(("RASH_PROCESS_TEST".into(), "child".into()));
        let result = spec.run().unwrap();
        assert_eq!(result.stdout.as_deref(), Some("child"));
        assert!(std::env::var("RASH_PROCESS_TEST").is_err());
    }

    #[test]
    fn nonzero_is_a_result_not_a_spawn_error() {
        let result = shell("echo bad >&2; exit 7").run().unwrap();
        assert!(!result.success());
        assert_eq!(result.rc(), 7);
        assert_eq!(result.stderr.as_deref(), Some("bad\n"));
    }

    #[test]
    fn managed_process_drains_large_output_before_exit() {
        let spec =
            shell("i=0; while [ $i -lt 20000 ]; do echo 01234567890123456789; i=$((i+1)); done");
        let result = spec.run().unwrap();
        assert!(result.success());
        assert!(result.stdout.unwrap().len() > 300_000);
    }

    #[test]
    fn large_stdin_and_captured_stdout_do_not_deadlock() {
        let payload = "x".repeat(256 * 1024);
        let mut spec = ProcessSpec::new("cat");
        spec.stdin = Some(payload.clone());
        let result = spec.run().unwrap();
        assert!(result.success());
        assert_eq!(result.stdout.as_deref(), Some(payload.as_str()));
    }

    #[test]
    fn unread_stdin_reports_rc_instead_of_broken_pipe() {
        let payload = "x".repeat(4 * 1024 * 1024);
        let mut spec = ProcessSpec::new("head");
        spec.args = vec!["-c1".into()];
        spec.stdin = Some(payload.clone());
        let result = spec.run().unwrap();
        assert_eq!(result.rc(), 0);
        assert_eq!(result.stdout.as_deref(), Some("x"));

        let mut spec = shell("exit 3");
        spec.stdin = Some(payload);
        assert_eq!(spec.run().unwrap().rc(), 3);
    }

    #[test]
    fn null_discards_output() {
        let mut spec = shell("printf hidden");
        spec.stdout = OutputMode::Null;
        let result = spec.run().unwrap();
        assert_eq!(result.stdout, None);
    }

    #[test]
    fn tee_captures_output() {
        let mut spec = shell("printf teed; printf teed-err >&2");
        spec.stdout = OutputMode::Tee;
        spec.stderr = OutputMode::Tee;
        let result = spec.run().unwrap();
        assert_eq!(result.stdout.as_deref(), Some("teed"));
        assert_eq!(result.stderr.as_deref(), Some("teed-err"));
    }

    #[test]
    fn inherit_does_not_capture() {
        let mut spec = shell("printf inherited");
        spec.stdout = OutputMode::Inherit;
        let result = spec.run().unwrap();
        assert!(result.success());
        assert_eq!(result.stdout, None);
    }

    /// Pid and process group of a running child, read with getpgid(2) (no `/proc`).
    fn child_pid_and_pgid(process_group: bool) -> (i32, i32) {
        let mut spec = ProcessSpec::new("sleep");
        spec.args = vec!["30".into()];
        spec.process_group = process_group;
        // spawn returns once the child exec'd, so its process group is already set.
        let mut process = spec.spawn_managed().unwrap();
        let pid = process.id() as i32;
        // SAFETY: getpgid(2) on our own unreaped child.
        let pgid = unsafe { libc::getpgid(pid) };
        process.kill_tree().unwrap();
        process.wait().unwrap();
        (pid, pgid)
    }

    #[test]
    fn sync_child_runs_in_rash_process_group() {
        let (_, pgid) = child_pid_and_pgid(false);
        // SAFETY: getpgrp(2) has no preconditions.
        assert_eq!(pgid, unsafe { libc::getpgrp() });
    }

    #[test]
    fn process_group_isolates_child() {
        let (pid, pgid) = child_pid_and_pgid(true);
        assert_eq!(pid, pgid);
    }

    #[test]
    fn finish_within_does_not_block_on_grandchild_holding_stdout() {
        let mut spec = shell("echo started; sleep 120 &");
        spec.process_group = true;
        let mut process = spec.spawn_managed().unwrap();
        let pgid = process.id() as i32;
        let start = Instant::now();
        let status = process.wait().unwrap();
        let result = process
            .finish_within(status, Duration::from_secs(1))
            .unwrap();
        // SAFETY: clean up the orphaned `sleep` in the job's process group.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
        // Returned after the grace period, not when the grandchild closed stdout.
        assert!(start.elapsed() < Duration::from_secs(60));
        assert_eq!(result.stdout.as_deref(), Some("started\n"));
    }
}
