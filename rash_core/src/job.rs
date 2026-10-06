use crate::error::Result;
use crate::process::{ProcessResult, SpawnedProcess};
use crate::signal;

use std::collections::HashMap;
use std::process::ExitStatus;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub type JobId = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Pending,
    Running,
    Finished,
    Failed,
}

#[derive(Debug, Clone)]
pub struct JobInfo {
    pub status: JobStatus,
    pub output: Option<String>,
    pub stderr: Option<String>,
    pub rc: Option<i32>,
    pub error: Option<String>,
    pub changed: bool,
    pub elapsed: Duration,
}

#[derive(Debug)]
pub struct Job {
    pub id: JobId,
    pub status: JobStatus,
    pub started_at: Instant,
    pub timeout: Option<Duration>,
    pub process: Option<SpawnedProcess>,
    pub output: Option<String>,
    pub stderr: Option<String>,
    pub rc: Option<i32>,
    pub error: Option<String>,
    pub changed: bool,
}

impl Job {
    pub fn new(id: JobId, timeout: Option<Duration>, process: SpawnedProcess) -> Self {
        Self {
            id,
            status: JobStatus::Running,
            started_at: Instant::now(),
            timeout,
            process: Some(process),
            output: None,
            stderr: None,
            rc: None,
            error: None,
            changed: false,
        }
    }

    pub fn is_timed_out(&self) -> bool {
        self.timeout
            .map(|timeout| self.started_at.elapsed() > timeout)
            .unwrap_or(false)
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

#[derive(Debug, Default)]
pub struct JobRegistry {
    jobs: HashMap<JobId, Job>,
    next_id: JobId,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn register(&mut self, timeout: Option<Duration>, process: SpawnedProcess) -> JobId {
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.insert(id, Job::new(id, timeout, process));
        id
    }

    pub fn get(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn get_mut(&mut self, id: JobId) -> Option<&mut Job> {
        self.jobs.get_mut(&id)
    }

    pub fn remove(&mut self, id: JobId) -> Option<Job> {
        self.jobs.remove(&id)
    }

    pub fn contains(&self, id: JobId) -> bool {
        self.jobs.contains_key(&id)
    }

    pub fn list(&self) -> Vec<JobId> {
        self.jobs.keys().copied().collect()
    }
}

pub static JOBS: LazyLock<Arc<Mutex<JobRegistry>>> =
    LazyLock::new(|| Arc::new(Mutex::new(JobRegistry::new())));

/// Grace period to drain output of an exited job before abandoning pipes still held open
/// by its background grandchildren (`sleep 100 &`, `nohup daemon &`).
const OUTPUT_GRACE: Duration = Duration::from_millis(200);

/// Lock the registry. A panic while holding the lock leaves plain data behind, so a
/// poisoned registry is still usable.
fn registry() -> MutexGuard<'static, JobRegistry> {
    JOBS.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn register_job(timeout: Option<Duration>, process: SpawnedProcess) -> JobId {
    signal::register_job_group(process.id());
    registry().register(timeout, process)
}

pub fn get_job(id: JobId) -> Option<JobStatus> {
    check_and_update_job_status(id);
    registry().get(id).map(|j| j.status.clone())
}

enum Completion {
    Exited(ExitStatus),
    TimedOut(Option<Duration>),
    Unknown(std::io::Error),
}

/// Take the process of a running job out of the registry if it needs completing.
///
/// The job stays `Running` with no process while it is completed outside the lock, so
/// concurrent callers skip it instead of blocking.
fn claim_completed_process(id: JobId) -> Option<(SpawnedProcess, Completion)> {
    let mut registry = registry();
    let job = registry.get_mut(id)?;
    if job.status != JobStatus::Running {
        return None;
    }
    let timed_out = job.is_timed_out();
    let completion = match job.process.as_mut()?.try_wait() {
        Ok(Some(status)) => Completion::Exited(status),
        Ok(None) if timed_out => Completion::TimedOut(job.timeout),
        Ok(None) => return None,
        Err(e) => Completion::Unknown(e),
    };
    let process = job.process.take()?;
    Some((process, completion))
}

#[derive(Default)]
struct JobOutcome {
    status: Option<JobStatus>,
    output: Option<String>,
    stderr: Option<String>,
    rc: Option<i32>,
    error: Option<String>,
    changed: bool,
}

impl JobOutcome {
    fn failed(error: String) -> Self {
        Self {
            status: Some(JobStatus::Failed),
            error: Some(error),
            ..Self::default()
        }
    }

    fn from_result(result: Result<ProcessResult>) -> Self {
        let result = match result {
            Ok(result) => result,
            Err(e) => return Self::failed(format!("Failed to collect process output: {e}")),
        };
        let rc = result.rc();
        let success = result.success();
        let error = (!success).then(|| {
            format!(
                "Process exited with code {rc}: {}",
                result.stderr.as_deref().unwrap_or_default().trim()
            )
        });
        Self {
            status: Some(if success {
                JobStatus::Finished
            } else {
                JobStatus::Failed
            }),
            output: result.stdout,
            stderr: result.stderr,
            rc: Some(rc),
            error,
            changed: true,
        }
    }

    fn apply(self, job: &mut Job) {
        job.status = self.status.unwrap_or(JobStatus::Failed);
        job.output = self.output;
        job.stderr = self.stderr;
        job.rc = self.rc;
        job.error = self.error;
        job.changed = self.changed;
    }
}

/// Complete a claimed job process without holding the registry lock.
fn complete(mut process: SpawnedProcess, completion: Completion) -> JobOutcome {
    let pgid = process.id();
    let outcome = match completion {
        Completion::Exited(status) => {
            JobOutcome::from_result(process.finish_within(status, OUTPUT_GRACE))
        }
        Completion::TimedOut(timeout) => {
            // Kill the whole process group so no grandchild survives the timeout.
            let _ = process.kill_tree();
            if let Ok(status) = process.wait() {
                let _ = process.finish_within(status, OUTPUT_GRACE);
            }
            JobOutcome::failed(format!("Job timed out after {timeout:?}"))
        }
        Completion::Unknown(e) => {
            JobOutcome::failed(format!("Failed to check process status: {e}"))
        }
    };
    signal::unregister_job_group(pgid);
    outcome
}

fn check_and_update_job_status(id: JobId) {
    let Some((process, completion)) = claim_completed_process(id) else {
        return;
    };
    let outcome = complete(process, completion);
    if let Some(job) = registry().get_mut(id) {
        outcome.apply(job);
    }
}

pub fn get_job_info(id: JobId) -> Option<JobInfo> {
    check_and_update_job_status(id);
    registry().get(id).map(|j| JobInfo {
        status: j.status.clone(),
        output: j.output.clone(),
        stderr: j.stderr.clone(),
        rc: j.rc,
        error: j.error.clone(),
        changed: j.changed,
        elapsed: j.elapsed(),
    })
}

pub fn update_job_status(
    id: JobId,
    status: JobStatus,
    output: Option<String>,
    error: Option<String>,
    changed: bool,
) -> bool {
    if let Some(job) = registry().get_mut(id) {
        job.status = status;
        job.output = output;
        job.error = error;
        job.changed = changed;
        if let Some(process) = job.process.take() {
            signal::unregister_job_group(process.id());
        }
        true
    } else {
        false
    }
}

/// Kill the whole process tree of a job and forget the job, for jobs nobody will wait
/// for. Returns whether the job existed.
pub fn kill_job(id: JobId) -> bool {
    let Some(job) = registry().remove(id) else {
        return false;
    };
    if let Some(mut process) = job.process {
        let _ = process.kill_tree();
        // Unregister before reaping: afterwards the process group id may be reused.
        signal::unregister_job_group(process.id());
        let _ = process.wait();
    }
    true
}

pub fn job_exists(id: JobId) -> bool {
    registry().contains(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessSpec;
    use std::fs::{File, OpenOptions};
    use std::io::{ErrorKind as IoErrorKind, Read};
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::thread;

    /// Generous deadline: under a loaded machine these processes can take a while.
    const LIMIT: Duration = Duration::from_secs(30);
    const POLL: Duration = Duration::from_millis(10);

    fn spawn(command: &str) -> SpawnedProcess {
        let mut spec = ProcessSpec::shell(command, "/bin/sh");
        spec.process_group = true;
        spec.spawn_managed().unwrap()
    }

    fn wait_until_done(job_id: JobId) -> JobInfo {
        let start = Instant::now();
        loop {
            let info = get_job_info(job_id).unwrap();
            if info.status != JobStatus::Running {
                return info;
            }
            assert!(start.elapsed() < LIMIT, "job {job_id} still running");
            thread::sleep(POLL);
        }
    }

    /// A FIFO held open for writing by the processes under test.
    ///
    /// They write a line once ready, and reading reports EOF only once every holder
    /// exited. Unlike checking a pid, this cannot be fooled by pid reuse or by how long
    /// a zombie waits to be reaped, and it needs no `/proc`.
    struct Lifeline {
        _dir: tempfile::TempDir,
        path: PathBuf,
        reader: File,
    }

    impl Lifeline {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("lifeline");
            nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRWXU).unwrap();
            // Non-blocking: opening does not wait for a writer and reads never block.
            let reader = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
                .unwrap();
            Self {
                _dir: dir,
                path,
                reader,
            }
        }

        /// Read once: `Some(true)` on data, `Some(false)` on EOF, `None` if still open.
        fn read(&mut self, line: &mut Vec<u8>) -> Option<bool> {
            let mut buffer = [0; 64];
            match self.reader.read(&mut buffer) {
                Ok(0) => Some(false),
                Ok(n) => {
                    line.extend_from_slice(&buffer[..n]);
                    Some(true)
                }
                Err(e) if e.kind() == IoErrorKind::WouldBlock => None,
                Err(e) => panic!("reading lifeline: {e}"),
            }
        }

        /// Wait until a holder wrote a full line.
        fn wait_ready(&mut self) {
            let start = Instant::now();
            let mut line = Vec::new();
            // EOF before the first holder opened the FIFO only means "not yet".
            while !line.ends_with(b"\n") {
                assert!(start.elapsed() < LIMIT, "lifeline holder never got ready");
                if self.read(&mut line) != Some(true) {
                    thread::sleep(POLL);
                }
            }
        }

        /// Wait until every holder exited.
        fn wait_released(&mut self) -> bool {
            let start = Instant::now();
            while start.elapsed() < LIMIT {
                match self.read(&mut Vec::new()) {
                    Some(false) => return true,
                    Some(true) => {}
                    None => thread::sleep(POLL),
                }
            }
            false
        }
    }

    #[test]
    fn test_job_registry() {
        assert!(JobRegistry::new().list().is_empty());
    }

    #[test]
    fn test_register_job_and_get_status() {
        let job_id = register_job(None, spawn("sleep 0.1"));
        wait_until_done(job_id);
        assert_eq!(get_job(job_id), Some(JobStatus::Finished));
    }

    #[test]
    fn test_get_job_info_updates_status() {
        let job_id = register_job(None, spawn("echo test_output"));
        let info = wait_until_done(job_id);
        assert_eq!(info.status, JobStatus::Finished);
        assert_eq!(info.rc, Some(0));
        assert!(info.output.unwrap().contains("test_output"));
    }

    #[test]
    fn test_job_large_output_does_not_deadlock() {
        let job_id = register_job(
            Some(LIMIT),
            spawn("i=0; while [ $i -lt 20000 ]; do echo abcdefghijklmnop; i=$((i+1)); done"),
        );
        let info = wait_until_done(job_id);
        assert_eq!(info.status, JobStatus::Finished);
        assert!(info.output.unwrap().len() > 300_000);
    }

    #[test]
    fn test_job_timeout() {
        let job_id = register_job(Some(Duration::from_millis(100)), spawn("sleep 30"));
        let info = wait_until_done(job_id);
        assert_eq!(info.status, JobStatus::Failed);
        assert!(info.error.unwrap().contains("timed out"));
    }

    #[test]
    fn test_kill_job_kills_process_tree_and_forgets_job() {
        let mut lifeline = Lifeline::new();
        let job_id = register_job(
            None,
            spawn(&format!(
                "exec 3>'{}'; (echo ready >&3; exec sleep 30) & wait",
                lifeline.path.display()
            )),
        );
        lifeline.wait_ready();

        assert!(kill_job(job_id));
        assert!(!job_exists(job_id));
        assert!(!kill_job(job_id));
        assert!(
            lifeline.wait_released(),
            "a process of the job tree survived"
        );
    }

    #[test]
    fn test_job_failed_on_nonzero_exit_preserves_status() {
        let job_id = register_job(None, spawn("echo bad >&2; exit 7"));
        let info = wait_until_done(job_id);
        assert_eq!(info.status, JobStatus::Failed);
        assert_eq!(info.rc, Some(7));
        assert!(info.stderr.unwrap().contains("bad"));
    }

    #[test]
    fn test_job_with_grandchild_holding_stdout_completes() {
        let process = spawn("echo started; sleep 120 &");
        let pgid = process.id() as i32;
        let job_id = register_job(Some(Duration::from_secs(60)), process);
        // Completes within LIMIT, long before the grandchild exits or the job times out.
        let info = wait_until_done(job_id);
        // Concurrent lookups must not block on the completed job either.
        assert!(job_exists(job_id));
        // SAFETY: clean up the orphaned `sleep` left in the job's process group.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
        assert_eq!(info.status, JobStatus::Finished);
        assert_eq!(info.output.as_deref(), Some("started\n"));
    }

    #[test]
    fn test_job_timeout_kills_process_tree() {
        let mut lifeline = Lifeline::new();
        let job_id = register_job(
            Some(Duration::from_millis(300)),
            spawn(&format!(
                "exec 3>'{}'; (echo ready >&3; exec sleep 30) & wait",
                lifeline.path.display()
            )),
        );
        // The backgrounded grandchild is running before the timeout can kill anything.
        lifeline.wait_ready();

        let info = wait_until_done(job_id);
        assert_eq!(info.status, JobStatus::Failed);
        assert!(info.error.unwrap().contains("timed out"));
        assert!(
            lifeline.wait_released(),
            "a process of the job tree survived the timeout"
        );
    }
}
