use crate::process::SpawnedProcess;

use std::collections::HashMap;
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

pub fn register_job(timeout: Option<Duration>, process: SpawnedProcess) -> JobId {
    JOBS.lock()
        .expect("Failed to lock job registry")
        .register(timeout, process)
}

pub fn get_job(id: JobId) -> Option<JobStatus> {
    check_and_update_job_status(id);
    JOBS.lock()
        .expect("Failed to lock job registry")
        .get(id)
        .map(|j| j.status.clone())
}

fn check_and_update_job_status(id: JobId) {
    let mut registry = JOBS.lock().expect("Failed to lock job registry");
    let Some(job) = registry.get_mut(id) else {
        return;
    };
    if job.status != JobStatus::Running {
        return;
    }

    let timed_out = job.is_timed_out();
    let timeout_val = job.timeout;
    let Some(process) = job.process.as_mut() else {
        return;
    };

    match process.try_wait() {
        Ok(Some(status)) => {
            let process = job.process.take().expect("process exists");
            match process.finish(status) {
                Ok(result) => {
                    let rc = result.rc();
                    let success = result.success();
                    let stdout = result.stdout;
                    let stderr = result.stderr;
                    job.rc = Some(rc);
                    job.output = stdout;
                    job.stderr = stderr.clone();
                    job.changed = true;
                    if success {
                        job.status = JobStatus::Finished;
                    } else {
                        job.status = JobStatus::Failed;
                        job.error = Some(format!(
                            "Process exited with code {rc}: {}",
                            stderr.unwrap_or_default().trim()
                        ));
                    }
                }
                Err(e) => {
                    job.status = JobStatus::Failed;
                    job.error = Some(format!("Failed to collect process output: {e}"));
                }
            }
        }
        Ok(None) if timed_out => {
            let mut process = job.process.take().expect("process exists");
            let _ = process.kill_tree();
            let status = process.wait();
            if let Ok(status) = status {
                let _ = process.finish(status);
            }
            job.status = JobStatus::Failed;
            job.error = Some(format!("Job timed out after {timeout_val:?}"));
        }
        Ok(None) => {}
        Err(e) => {
            job.status = JobStatus::Failed;
            job.error = Some(format!("Failed to check process status: {e}"));
            job.process = None;
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
        job.process = None;
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
    use crate::process::ProcessSpec;
    use std::thread;

    fn spawn(command: &str) -> SpawnedProcess {
        ProcessSpec::shell(command, "/bin/sh")
            .spawn_managed()
            .unwrap()
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
}
