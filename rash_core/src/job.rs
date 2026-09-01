use crate::process::kill_process_tree;

use std::collections::HashMap;
use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::process::Child;
use std::sync::{Arc, LazyLock, Mutex};
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
    pub child: Option<Child>,
    pub output: Option<String>,
    pub stderr: Option<String>,
    pub rc: Option<i32>,
    pub error: Option<String>,
    pub changed: bool,
}

impl Job {
    pub fn new(id: JobId, timeout: Option<Duration>, child: Child) -> Self {
        Job {
            id,
            status: JobStatus::Running,
            started_at: Instant::now(),
            timeout,
            child: Some(child),
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

    pub fn register(&mut self, timeout: Option<Duration>, child: Child) -> JobId {
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.insert(id, Job::new(id, timeout, child));
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

pub fn register_job(timeout: Option<Duration>, child: Child) -> JobId {
    JOBS.lock()
        .expect("Failed to lock job registry")
        .register(timeout, child)
}

pub fn get_job(id: JobId) -> Option<JobStatus> {
    check_and_update_job_status(id);
    JOBS.lock()
        .expect("Failed to lock job registry")
        .get(id)
        .map(|j| j.status.clone())
}

fn exit_code(status: &std::process::ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(-1)
}

fn check_and_update_job_status(id: JobId) {
    let mut registry = JOBS.lock().expect("Failed to lock job registry");
    if let Some(job) = registry.get_mut(id) {
        if job.status != JobStatus::Running {
            return;
        }

        let timed_out = job.is_timed_out();
        let timeout_val = job.timeout;

        if let Some(ref mut child) = job.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    if let Some(ref mut stdout_handle) = child.stdout {
                        let _ = stdout_handle.read_to_string(&mut stdout);
                    }
                    if let Some(ref mut stderr_handle) = child.stderr {
                        let _ = stderr_handle.read_to_string(&mut stderr);
                    }

                    let rc = exit_code(&status);
                    job.rc = Some(rc);
                    job.output = (!stdout.is_empty()).then_some(stdout);
                    job.stderr = (!stderr.is_empty()).then_some(stderr.clone());
                    job.changed = true;

                    if status.success() {
                        job.status = JobStatus::Finished;
                    } else {
                        job.status = JobStatus::Failed;
                        job.error = Some(format!(
                            "Process exited with code {rc}: {}",
                            stderr.trim()
                        ));
                    }
                    job.child = None;
                }
                Ok(None) if timed_out => {
                    let _ = kill_process_tree(child);
                    let _ = child.wait();
                    job.status = JobStatus::Failed;
                    job.error = Some(format!("Job timed out after {timeout_val:?}"));
                    job.child = None;
                }
                Ok(None) => {}
                Err(e) => {
                    job.status = JobStatus::Failed;
                    job.error = Some(format!("Failed to check process status: {e}"));
                    job.child = None;
                }
            }
        }
    }
}

pub fn get_job_info(id: JobId) -> Option<JobInfo> {
    check_and_update_job_status(id);
    JOBS.lock()
        .expect("Failed to lock job registry")
        .get(id)
        .map(|j| JobInfo {
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
    let mut registry = JOBS.lock().expect("Failed to lock job registry");
    if let Some(job) = registry.get_mut(id) {
        job.status = status;
        job.output = output;
        job.error = error;
        job.changed = changed;
        job.child = None;
        true
    } else {
        false
    }
}

pub fn job_exists(id: JobId) -> bool {
    JOBS.lock()
        .expect("Failed to lock job registry")
        .contains(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::thread;

    #[test]
    fn test_job_registry() {
        assert!(JobRegistry::new().list().is_empty());
    }

    #[test]
    fn test_register_job_and_get_status() {
        let mut command = Command::new("sleep");
        command.arg("0.1").stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command.spawn().unwrap();
        let job_id = register_job(None, child);
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
        let mut command = Command::new("echo");
        command.arg("test_output").stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let job_id = register_job(None, command.spawn().unwrap());
        let mut info = get_job_info(job_id);
        for _ in 0..20 {
            if info.as_ref().is_some_and(|i| i.status == JobStatus::Finished) {
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
    fn test_job_timeout() {
        let mut command = Command::new("sleep");
        command.arg("10").stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let job_id = register_job(Some(Duration::from_millis(100)), command.spawn().unwrap());
        thread::sleep(Duration::from_millis(200));
        let info = get_job_info(job_id).unwrap();
        assert_eq!(info.status, JobStatus::Failed);
        assert!(info.error.unwrap().contains("timed out"));
    }

    #[test]
    fn test_job_failed_on_nonzero_exit_preserves_status() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("echo bad >&2; exit 7")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let job_id = register_job(None, command.spawn().unwrap());
        thread::sleep(Duration::from_millis(50));
        let info = get_job_info(job_id).unwrap();
        assert_eq!(info.status, JobStatus::Failed);
        assert_eq!(info.rc, Some(7));
        assert!(info.stderr.unwrap().contains("bad"));
    }
}
