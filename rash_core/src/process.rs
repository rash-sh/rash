use crate::error::{Error, ErrorKind, Result};

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread::{self, JoinHandle};

#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;

#[cfg(unix)]
use std::os::unix::process::{CommandExt, ExitStatusExt};

/// Controls how a child process stream is handled.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Capture the stream and make it available in the module result.
    #[default]
    Capture,
    /// Connect the child stream directly to Rash's stream.
    Inherit,
    /// Discard the stream.
    Null,
    /// Stream output live while also retaining it in the module result.
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

/// A normalized process definition shared by command-like modules and async execution.
#[derive(Clone, Debug)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub chdir: Option<String>,
    pub stdin: Option<String>,
    pub stdout: OutputMode,
    pub stderr: OutputMode,
    /// Start the process in a new process group so signals/timeouts can target its whole tree.
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
            process_group: true,
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
        if self.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::inherit());
        }
        command.stdout(self.stdout.stdio());
        command.stderr(self.stderr.stdio());

        #[cfg(unix)]
        if self.process_group {
            command.process_group(0);
        }

        command
    }

    /// Spawn without waiting. Callers can register the child in the async job registry.
    pub fn spawn(&self) -> Result<Child> {
        let mut command = self.command();
        trace!("spawn process: {:?} {:?}", self.program, self.args);
        let mut child = command
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        write_stdin(&mut child, self.stdin.as_deref())?;
        Ok(child)
    }

    /// Run the process to completion, with concurrent draining of captured streams.
    pub fn run(&self) -> Result<ProcessResult> {
        let mut child = self.spawn()?;
        let process_group = self.process_group.then_some(child.id() as i32);
        let _signal_guard = process_group.map(SignalForwardGuard::new);

        let stdout_reader = if self.stdout.is_piped() {
            child
                .stdout
                .take()
                .map(|reader| spawn_reader(reader, self.stdout == OutputMode::Tee, false))
        } else {
            None
        };
        let stderr_reader = if self.stderr.is_piped() {
            child
                .stderr
                .take()
                .map(|reader| spawn_reader(reader, self.stderr == OutputMode::Tee, true))
        } else {
            None
        };

        let status = child
            .wait()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        let stdout = join_reader(stdout_reader)?;
        let stderr = join_reader(stderr_reader)?;

        Ok(ProcessResult {
            status,
            stdout: bytes_to_string(stdout),
            stderr: bytes_to_string(stderr),
        })
    }

    /// Replace the Rash process with this command. This never returns on success.
    #[cfg(unix)]
    pub fn replace(&self) -> Error {
        let mut command = self.command();
        // Once Rash is replaced there is no parent left to forward signals, so keep the
        // replacement in the current process group rather than creating a new one.
        let error = command.exec();
        Error::new(ErrorKind::SubprocessFail, error)
    }
}

fn write_stdin(child: &mut Child, stdin: Option<&str>) -> Result<()> {
    if let Some(data) = stdin
        && let Some(mut handle) = child.stdin.take()
    {
        handle
            .write_all(data.as_bytes())
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    }
    Ok(())
}

fn spawn_reader<R>(mut reader: R, tee: bool, stderr: bool) -> JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut collected = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            collected.extend_from_slice(&buffer[..read]);
            if tee {
                if stderr {
                    let mut out = std::io::stderr().lock();
                    out.write_all(&buffer[..read])?;
                    out.flush()?;
                } else {
                    let mut out = std::io::stdout().lock();
                    out.write_all(&buffer[..read])?;
                    out.flush()?;
                }
            }
        }
        Ok(collected)
    })
}

fn join_reader(
    handle: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
) -> Result<Option<Vec<u8>>> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| Error::new(ErrorKind::SubprocessFail, "process output reader panicked"))?
            .map(Some)
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e)),
        None => Ok(None),
    }
}

fn bytes_to_string(bytes: Option<Vec<u8>>) -> Option<String> {
    bytes.and_then(|bytes| {
        if bytes.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
    })
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

    pub fn rc(&self) -> i32 {
        if let Some(code) = self.status.code() {
            return code;
        }
        #[cfg(unix)]
        if let Some(signal) = self.status.signal() {
            return 128 + signal;
        }
        -1
    }
}

#[cfg(unix)]
static ACTIVE_PROCESS_GROUP: AtomicI32 = AtomicI32::new(0);

#[cfg(unix)]
extern "C" fn forward_signal(signal: libc::c_int) {
    let pgid = ACTIVE_PROCESS_GROUP.load(Ordering::Relaxed);
    if pgid > 0 {
        // SAFETY: kill(2) is async-signal-safe. A negative pid addresses the process group.
        unsafe {
            libc::kill(-pgid, signal);
        }
    }
}

struct SignalForwardGuard {
    #[cfg(unix)]
    old_sigint: libc::sighandler_t,
    #[cfg(unix)]
    old_sigterm: libc::sighandler_t,
}

impl SignalForwardGuard {
    fn new(pgid: i32) -> Self {
        #[cfg(unix)]
        {
            ACTIVE_PROCESS_GROUP.store(pgid, Ordering::SeqCst);
            // SAFETY: handlers are restored by Drop and only perform async-signal-safe work.
            let old_sigint = unsafe { libc::signal(libc::SIGINT, forward_signal as libc::sighandler_t) };
            let old_sigterm = unsafe { libc::signal(libc::SIGTERM, forward_signal as libc::sighandler_t) };
            return Self {
                old_sigint,
                old_sigterm,
            };
        }

        #[cfg(not(unix))]
        {
            let _ = pgid;
            Self {}
        }
    }
}

impl Drop for SignalForwardGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            ACTIVE_PROCESS_GROUP.store(0, Ordering::SeqCst);
            // SAFETY: restore the exact handlers that were active before this process run.
            unsafe {
                libc::signal(libc::SIGINT, self.old_sigint);
                libc::signal(libc::SIGTERM, self.old_sigterm);
            }
        }
    }
}

/// Kill the whole process group when possible, falling back to Child::kill.
pub fn kill_process_tree(child: &mut Child) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let pgid = child.id() as i32;
        // SAFETY: the child was started with process_group(0), so its pid is the pgid.
        let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };
        if result == 0 {
            return Ok(());
        }
    }
    child.kill()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout_and_status() {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".into(), "printf hello".into()];
        let result = spec.run().unwrap();
        assert!(result.success());
        assert_eq!(result.rc(), 0);
        assert_eq!(result.stdout.as_deref(), Some("hello"));
    }

    #[test]
    fn nonzero_is_a_result_not_a_spawn_error() {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".into(), "echo bad >&2; exit 7".into()];
        let result = spec.run().unwrap();
        assert!(!result.success());
        assert_eq!(result.rc(), 7);
        assert_eq!(result.stderr.as_deref(), Some("bad\n"));
    }

    #[test]
    fn null_discards_output() {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".into(), "printf hidden".into()];
        spec.stdout = OutputMode::Null;
        let result = spec.run().unwrap();
        assert_eq!(result.stdout, None);
    }
}
