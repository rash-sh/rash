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

#[derive(Clone, Debug)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub chdir: Option<String>,
    pub stdin: Option<String>,
    pub stdout: OutputMode,
    pub stderr: OutputMode,
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
        command.stdin(if self.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        });
        command.stdout(self.stdout.stdio());
        command.stderr(self.stderr.stdio());

        #[cfg(unix)]
        if self.process_group {
            command.process_group(0);
        }

        command
    }

    /// Spawn a process and immediately start draining all piped streams.
    /// This is safe for verbose long-running and asynchronous commands.
    pub fn spawn_managed(&self) -> Result<SpawnedProcess> {
        let mut command = self.command();
        trace!("spawn process: {:?} {:?}", self.program, self.args);
        let mut child = command
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        write_stdin(&mut child, self.stdin.as_deref())?;

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

        Ok(SpawnedProcess {
            child,
            stdout_reader,
            stderr_reader,
            process_group: self.process_group,
        })
    }

    pub fn run(&self) -> Result<ProcessResult> {
        let mut process = self.spawn_managed()?;
        let _signal_guard = process
            .process_group
            .then_some(process.id() as i32)
            .map(SignalForwardGuard::new);
        let status = process.wait()?;
        process.finish(status)
    }

    #[cfg(unix)]
    pub fn replace(&self) -> Error {
        let mut spec = self.clone();
        spec.process_group = false;
        let mut command = spec.command();
        let error = command.exec();
        Error::new(ErrorKind::SubprocessFail, error)
    }
}

pub struct SpawnedProcess {
    child: Child,
    stdout_reader: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    stderr_reader: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
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

    pub fn kill_tree(&mut self) -> std::io::Result<()> {
        if self.process_group {
            #[cfg(unix)]
            {
                let pgid = self.child.id() as i32;
                // SAFETY: ProcessSpec created this child with process_group(0).
                let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };
                if result == 0 {
                    return Ok(());
                }
            }
        }
        self.child.kill()
    }

    pub fn finish(mut self, status: ExitStatus) -> Result<ProcessResult> {
        // Close stdin in case a caller retained it. ProcessSpec normally closes it after writing.
        drop(self.child.stdin.take());
        let stdout = join_reader(self.stdout_reader.take())?;
        let stderr = join_reader(self.stderr_reader.take())?;
        Ok(ProcessResult {
            status,
            stdout: bytes_to_string(stdout),
            stderr: bytes_to_string(stderr),
        })
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

fn join_reader(handle: Option<JoinHandle<std::io::Result<Vec<u8>>>>) -> Result<Option<Vec<u8>>> {
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
        (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned())
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
        // SAFETY: kill(2) is async-signal-safe; a negative pid addresses a process group.
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
            // SAFETY: restored in Drop; handler only invokes async-signal-safe kill(2).
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
            // SAFETY: restore the exact previous handlers.
            unsafe {
                libc::signal(libc::SIGINT, self.old_sigint);
                libc::signal(libc::SIGTERM, self.old_sigterm);
            }
        }
    }
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
    fn managed_process_drains_large_output_before_exit() {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec![
            "-c".into(),
            "i=0; while [ $i -lt 20000 ]; do echo 01234567890123456789; i=$((i+1)); done".into(),
        ];
        let result = spec.run().unwrap();
        assert!(result.success());
        assert!(result.stdout.unwrap().len() > 300_000);
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
