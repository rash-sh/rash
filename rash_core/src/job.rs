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

pub fn job_exists(id: JobId) -> bool {
    registry().contains(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessSpec;
    use std::thread;

    fn spawn(command: &str) -> SpawnedProcess {
        let mut spec = ProcessSpec::shell(command, "/bin/sh");
        spec.process_group = true;
        spec.spawn_managed().unwrap()
    }

    fn wait_until_done(job_id: JobId, limit: Duration) -> JobInfo {
        let start = Instant::now();
        loop {
            let info = get_job_info(job_id).unwrap();
            if info.status != JobStatus::Running || start.elapsed() > limit {
                return info;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn process_alive(pid: i32) -> bool {
        // A zombie is already dead, it only waits for its new parent to reap it.
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                stat.rsplit_once(") ")
                    .map(|(_, rest)| !rest.starts_with('Z'))
            })
            .unwrap_or(false)
    }

    #[test]
    fn test_job_registry() {
        assert!(JobRegistry::new().list().is_empty());
    }

    #[test]
    fn test_register_job_and_get_status() {
        let job_id = register_job(None, spawn("sleep 0.1"));
        let mut status = get_job(job_id);
        for _ in 0..20 {
            if status == Some(JobStatus::Finished) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
            status = get_job(job_id);
        }
        assert_eq!(status, Some(JobStatus::Finished));
    }

    #[test]
    fn test_get_job_info_updates_status() {
        let job_id = register_job(None, spawn("echo test_output"));
        let mut info = get_job_info(job_id);
        for _ in 0..20 {
            if info
                .as_ref()
                .is_some_and(|i| i.status == JobStatus::Finished)
            {
                break;
            }
            thread::sleep(Duration::from_millis(50));
            info = get_job_info(job_id);
        }
        let info = info.unwrap();
        assert_eq!(info.rc, Some(0));
        assert!(info.output.unwrap().contains("test_output"));
    }

    #[test]
    fn test_job_large_output_does_not_deadlock() {
        let job_id = register_job(
            Some(Duration::from_secs(5)),
            spawn("i=0; while [ $i -lt 20000 ]; do echo abcdefghijklmnop; i=$((i+1)); done"),
        );
        let mut info = get_job_info(job_id);
        for _ in 0..100 {
            if info
                .as_ref()
                .is_some_and(|i| i.status != JobStatus::Running)
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
            info = get_job_info(job_id);
        }
        let info = info.unwrap();
        assert_eq!(info.status, JobStatus::Finished);
        assert!(info.output.unwrap().len() > 300_000);
    }

    #[test]
    fn test_job_timeout() {
        let job_id = register_job(Some(Duration::from_millis(100)), spawn("sleep 10"));
        thread::sleep(Duration::from_millis(200));
        let info = get_job_info(job_id).unwrap();
        assert_eq!(info.status, JobStatus::Failed);
        assert!(info.error.unwrap().contains("timed out"));
    }

    #[test]
    fn test_job_failed_on_nonzero_exit_preserves_status() {
        let job_id = register_job(None, spawn("echo bad >&2; exit 7"));
        thread::sleep(Duration::from_millis(50));
        let info = get_job_info(job_id).unwrap();
        assert_eq!(info.status, JobStatus::Failed);
        assert_eq!(info.rc, Some(7));
        assert!(info.stderr.unwrap().contains("bad"));
    }

    #[test]
    fn test_job_with_grandchild_holding_stdout_completes() {
        let process = spawn("echo started; sleep 30 &");
        let pgid = process.id() as i32;
        let start = Instant::now();
        let job_id = register_job(Some(Duration::from_secs(60)), process);
        let info = wait_until_done(job_id, Duration::from_secs(5));
        // Concurrent lookups must not block on the completed job either.
        assert!(job_exists(job_id));
        // SAFETY: clean up the orphaned `sleep` left in the job's process group.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(info.status, JobStatus::Finished);
        assert_eq!(info.output.as_deref(), Some("started\n"));
    }

    #[test]
    fn test_job_timeout_kills_process_tree() {
        let pid_file = tempfile::NamedTempFile::new().unwrap();
        let path = pid_file.path().display().to_string();
        let job_id = register_job(
            Some(Duration::from_millis(300)),
            spawn(&format!("sleep 30 & echo $! > {path}; wait")),
        );
        let start = Instant::now();
        let grandchild = loop {
            let content = std::fs::read_to_string(&path).unwrap();
            if let Ok(pid) = content.trim().parse::<i32>() {
                break pid;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(20));
        };
        assert!(process_alive(grandchild));

        let info = wait_until_done(job_id, Duration::from_secs(5));
        assert_eq!(info.status, JobStatus::Failed);
        assert!(info.error.unwrap().contains("timed out"));
        let start = Instant::now();
        while process_alive(grandchild) && start.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !process_alive(grandchild),
            "grandchild {grandchild} survived"
        );
    }
}
