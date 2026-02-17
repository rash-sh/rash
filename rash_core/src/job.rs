use std::collections::HashMap;
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
            error: None,
            changed: false,
        }
    }

    pub fn is_timed_out(&self) -> bool {
        if let Some(timeout) = self.timeout {
            self.started_at.elapsed() > timeout
        } else {
            false
        }
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
        JobRegistry {
            jobs: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn register(&mut self, timeout: Option<Duration>, child: Child) -> JobId {
        let id = self.next_id;
        self.next_id += 1;
        let job = Job::new(id, timeout, child);
        self.jobs.insert(id, job);
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
    let mut registry = JOBS.lock().expect("Failed to lock job registry");
    registry.register(timeout, child)
}

pub fn get_job(id: JobId) -> Option<JobStatus> {
    let registry = JOBS.lock().expect("Failed to lock job registry");
    registry.get(id).map(|j| j.status.clone())
}

pub fn get_job_info(id: JobId) -> Option<JobInfo> {
    let registry = JOBS.lock().expect("Failed to lock job registry");
    registry.get(id).map(|j| JobInfo {
        status: j.status.clone(),
        output: j.output.clone(),
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
    let registry = JOBS.lock().expect("Failed to lock job registry");
    registry.contains(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_registry() {
        let registry = JobRegistry::new();
        assert!(registry.list().is_empty());
    }
}
