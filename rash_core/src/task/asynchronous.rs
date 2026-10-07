//! Async tasks (`async` / `poll`): processes of modules like command and shell started as
//! background jobs. They follow the same module params, check mode and become semantics as
//! synchronous executions.
use crate::context::BecomeMethod;
use crate::error::{Error, ErrorKind, Result};
use crate::job::{JobInfo, JobStatus, get_job_info, kill_job, register_job};
use crate::modules::ModuleResult;
use crate::process::{ProcessPlan, ProcessSpec};
use crate::task::control::Accumulated;
use crate::task::{BecomeUser, Task, TaskExecResult};

use std::thread;
use std::time::Duration;

use minijinja::{Value, context};
use nix::unistd::Uid;
use serde_norway::Value as YamlValue;

/// How an async task (or loop item) started.
enum AsyncStart {
    /// `when` evaluated to false.
    Skipped,
    /// Nothing runs in background: check mode, or a `creates`/`removes` condition met.
    Done(ModuleResult),
    Started(u64),
}

/// Result of a finished async job or of an item that did not need one.
enum JobOutcome {
    Completed(ModuleResult),
    /// The job could not run or finish (spawn failure, timeout): no exit status.
    Broken(Error),
}

fn job_module_result(info: &JobInfo) -> Result<ModuleResult> {
    let extra = serde_norway::value::to_value(json!({
        "rc": info.rc,
        "stderr": info.stderr.clone().unwrap_or_default(),
        "failed": info.status == JobStatus::Failed,
    }))?;
    Ok(ModuleResult::new(
        info.changed,
        Some(extra),
        info.output.clone(),
    ))
}

/// The outcome of a job once it ended, or `None` while it runs.
fn finished_job(job_id: u64) -> Result<Option<JobOutcome>> {
    let info = get_job_info(job_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("Job {job_id} not found")))?;
    Ok(match info.status {
        JobStatus::Running | JobStatus::Pending => None,
        JobStatus::Failed if info.rc.is_none() => Some(JobOutcome::Broken(Error::new(
            ErrorKind::SubprocessFail,
            info.error
                .unwrap_or_else(|| format!("async job {job_id} failed")),
        ))),
        JobStatus::Finished | JobStatus::Failed => {
            Some(JobOutcome::Completed(job_module_result(&info)?))
        }
    })
}

impl Task {
    fn get_poll_interval(&self) -> u64 {
        self.poll.unwrap_or(0)
    }

    fn poll_sleep(&self) -> Duration {
        Duration::from_secs(self.get_poll_interval().max(1))
    }

    /// Apply `become` to an async process: run it directly as the become user.
    fn apply_async_become(&self, spec: &mut ProcessSpec) -> Result<()> {
        if !self.runs_with_become() {
            return Ok(());
        }
        if self.become_method == BecomeMethod::Sudo {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "async tasks do not support become_method sudo: use become_method syscall",
            ));
        }
        let user = self.resolve_become_user()?;
        if user.uid != Uid::current() {
            spec.user = Some(BecomeUser::from(&user).process_user()?);
        }
        Ok(())
    }

    fn spawn_async_job(&self, mut spec: ProcessSpec, vars: &Value) -> Result<u64> {
        spec.env = self.render_environment(&self.extend_vars(vars.clone())?)?;
        // Async jobs get their own process group: timeouts and interrupts kill the tree.
        spec.process_group = true;
        self.apply_async_become(&mut spec)?;
        let process = spec.spawn_managed()?;
        let job_id = register_job(self.r#async.map(Duration::from_secs), process);
        info!(target: "async", "Started async job {job_id}");
        Ok(job_id)
    }

    fn start_async(&self, vars: &Value) -> Result<AsyncStart> {
        if !self.is_exec(vars)? {
            return Ok(AsyncStart::Skipped);
        }
        let rendered_params = self.render_params(vars.clone())?;
        match self.module.plan_process(rendered_params, self.check_mode)? {
            ProcessPlan::Done(result) => Ok(AsyncStart::Done(result)),
            ProcessPlan::Run(spec) => Ok(AsyncStart::Started(self.spawn_async_job(spec, vars)?)),
            ProcessPlan::Replace(_) => Err(Error::new(
                ErrorKind::InvalidData,
                "transfer_pid cannot be combined with async execution",
            )),
        }
    }

    fn wait_job(&self, job_id: u64) -> Result<JobOutcome> {
        loop {
            if let Some(outcome) = finished_job(job_id)? {
                return Ok(outcome);
            }
            thread::sleep(self.poll_sleep());
        }
    }

    fn finish_outcome(&self, outcome: JobOutcome, vars: &Value) -> Result<TaskExecResult> {
        match outcome {
            JobOutcome::Completed(result) => self.finalize_module_result(result, None, vars, false),
            JobOutcome::Broken(error) => Ok(self.module_error_result(error)),
        }
    }

    pub(super) fn exec_async_single(&self, vars: Value) -> Result<TaskExecResult> {
        let extended = self.extend_vars(vars.clone())?;
        let job_id = match self.start_async(&vars)? {
            AsyncStart::Skipped => return Ok(TaskExecResult::new(false, None)),
            AsyncStart::Done(result) => {
                return self.finalize_module_result(result, None, &extended, false);
            }
            AsyncStart::Started(job_id) => job_id,
        };
        if self.get_poll_interval() == 0 {
            let extra = serde_norway::value::to_value(json!({
                "rash_job_id": job_id,
                "failed": false,
            }))?;
            let output = Some(format!("async job started: {job_id}"));
            let result = ModuleResult::new(true, Some(extra), output);
            return self.finalize_module_result(result, None, &extended, false);
        }
        let outcome = self.wait_job(job_id)?;
        self.finish_outcome(outcome, &extended)
    }

    /// Start every loop item before waiting for any of them. If an item cannot start, the
    /// jobs of the previous items are killed: nobody would wait for them.
    fn start_async_items(&self, vars: &Value) -> Result<Vec<ItemResult>> {
        let mut items: Vec<ItemResult> = Vec::new();
        for item in self.render_iterator(vars.clone())? {
            let item_vars = context! {item => &item, ..vars.clone()};
            let started = self.start_async(&item_vars).inspect_err(|_| {
                for job_id in items.iter().filter_map(|entry| entry.job_id) {
                    kill_job(job_id);
                }
            })?;
            match started {
                AsyncStart::Skipped => {}
                AsyncStart::Done(result) => items.push(ItemResult {
                    item,
                    job_id: None,
                    outcome: Some(JobOutcome::Completed(result)),
                }),
                AsyncStart::Started(job_id) => items.push(ItemResult {
                    item,
                    job_id: Some(job_id),
                    outcome: None,
                }),
            }
        }
        Ok(items)
    }

    /// Wait until every started job ended, polling all of them each interval.
    fn wait_async_items(&self, mut items: Vec<ItemResult>) -> Result<Vec<ItemResult>> {
        loop {
            for entry in items.iter_mut().filter(|entry| entry.outcome.is_none()) {
                if let Some(job_id) = entry.job_id {
                    entry.outcome = finished_job(job_id)?;
                }
            }
            if items.iter().all(|entry| entry.outcome.is_some()) {
                return Ok(items);
            }
            thread::sleep(self.poll_sleep());
        }
    }

    pub(super) fn exec_parallel_loop(&self, vars: Value) -> Result<TaskExecResult> {
        let extended = self.extend_vars(vars.clone())?;
        let items = self.start_async_items(&vars)?;
        let job_ids: Vec<u64> = items.iter().filter_map(|entry| entry.job_id).collect();
        if self.get_poll_interval() == 0 {
            let changed = !job_ids.is_empty()
                || items.iter().any(|entry| {
                    matches!(&entry.outcome, Some(JobOutcome::Completed(result)) if result.get_changed())
                });
            let extra = serde_norway::value::to_value(json!({
                "rash_job_ids": job_ids,
                "failed": false,
            }))?;
            let result = ModuleResult::new(changed, Some(extra), None);
            return self.finalize_module_result(result, None, &extended, false);
        }
        let items = self.wait_async_items(items)?;
        self.finish_async_items(items, &vars)
    }

    /// Fold the ended items into the loop result: each item is finalized like a synchronous
    /// one (`changed_when`, `failed_when`, `register`) with its `item` in scope.
    fn finish_async_items(&self, items: Vec<ItemResult>, vars: &Value) -> Result<TaskExecResult> {
        let mut accumulated = Accumulated::default();
        for entry in items {
            let Some(outcome) = entry.outcome else {
                continue;
            };
            let item_vars = self.extend_vars(context! {item => &entry.item, ..vars.clone()})?;
            let result = self.finish_outcome(outcome, &item_vars)?;
            accumulated.add_item(self, &entry.item, result);
        }
        Ok(accumulated.into_loop_result(self))
    }
}

/// An item of an async loop and, once it ended, its outcome.
struct ItemResult {
    item: YamlValue,
    job_id: Option<u64>,
    outcome: Option<JobOutcome>,
}

#[cfg(test)]
mod tests {
    use crate::context::GlobalParams;
    use crate::error::ErrorKind;
    use crate::task::Task;

    use minijinja::context;
    use nix::unistd::Uid;
    use serde_norway::Value as YamlValue;
    use tempfile::tempdir;

    fn exec(yaml: &str) -> crate::error::Result<crate::task::TaskExecResult> {
        let yaml: YamlValue = serde_norway::from_str(yaml).unwrap();
        let global_params = GlobalParams::default();
        Task::new(&yaml, &global_params)?.exec(context! {})
    }

    #[test]
    fn check_mode_reports_change_without_running() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker");
        for (attrs, poll) in [("loop: [a, b]", 1), ("", 1), ("", 0)] {
            let result = exec(&format!(
                "command:\n  argv: [touch, {}]\nasync: 10\npoll: {poll}\ncheck_mode: true\n{attrs}",
                marker.display()
            ))
            .unwrap();
            assert!(result.get_changed(), "{attrs} poll {poll}");
            assert!(!marker.exists(), "{attrs} poll {poll}");
        }
    }

    #[test]
    fn shell_creates_skips_async_job() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker");
        let result = exec(&format!(
            "shell:\n  cmd: echo ran > {0}.new\n  creates: {1}\nasync: 10\npoll: 1",
            marker.display(),
            dir.path().display()
        ))
        .unwrap();
        assert!(!result.get_changed());
        assert!(!dir.path().join("marker.new").exists());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let error = exec("command:\n  cmd: echo hi\n  bogus: 1\nasync: 10\npoll: 1").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn transfer_pid_is_rejected() {
        let error =
            exec("command:\n  cmd: echo hi\n  transfer_pid: true\nasync: 10\npoll: 1").unwrap_err();
        assert!(error.to_string().contains("transfer_pid"));
    }

    #[test]
    fn become_sudo_is_rejected() {
        let error = exec("command: echo hi\nasync: 10\npoll: 1\nbecome: true\nbecome_method: sudo")
            .unwrap_err();
        assert!(error.to_string().contains("become_method sudo"), "{error}");
    }

    #[test]
    fn become_syscall_runs_job_as_become_user() {
        if Uid::effective().is_root() {
            return;
        }
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker");
        let result = exec(&format!(
            "command:\n  argv: [touch, {}]\nasync: 10\npoll: 1\nbecome: true\nbecome_user: nobody",
            marker.display()
        ));
        // Switching to another user needs privileges: the job must not run as ourselves.
        assert!(result.is_err());
        assert!(!marker.exists());
    }

    /// Root-only (see `running_as_root` in the CLI tests): the job runs with the become
    /// user's uid, gid and supplementary groups, not root's.
    #[test]
    fn test_as_root_become_syscall_job_has_become_user_groups() {
        use crate::task::privilege::supplementary_groups;
        use nix::unistd::User;
        use std::collections::BTreeSet;

        if !Uid::effective().is_root() {
            eprintln!(
                "test_as_root_become_syscall_job_has_become_user_groups: skipped: requires root"
            );
            return;
        }
        // Under sudo, the invoking user: unlike nobody, it usually has supplementary groups.
        let name = std::env::var("SUDO_USER").unwrap_or_else(|_| "nobody".to_owned());
        let user = User::from_name(&name).unwrap().unwrap();
        let expected: BTreeSet<u32> = supplementary_groups(&name, user.gid)
            .unwrap()
            .into_iter()
            .chain([user.gid.as_raw()])
            .collect();

        let result = exec(&format!(
            "command: id -u; id -g; id -G\nasync: 10\npoll: 1\nbecome: true\nbecome_user: {name}\nregister: out"
        ))
        .unwrap();
        let stdout = result
            .get_vars()
            .unwrap()
            .get_attr("out")
            .unwrap()
            .get_attr("stdout")
            .unwrap()
            .to_string();
        let mut lines = stdout.lines();
        assert_eq!(lines.next().unwrap(), user.uid.to_string(), "{stdout}");
        assert_eq!(lines.next().unwrap(), user.gid.to_string(), "{stdout}");
        let groups: BTreeSet<u32> = lines
            .next()
            .unwrap()
            .split_whitespace()
            .map(|gid| gid.parse().unwrap())
            .collect();
        assert_eq!(groups, expected, "{stdout}");
    }

    #[test]
    fn parallel_loop_collects_item_results() {
        let result = exec(
            "command:\n  argv: [echo, '{{ item }}']\nloop: [one, two]\nasync: 10\npoll: 1\nregister: out",
        )
        .unwrap();
        let registered = result.get_vars().unwrap().get_attr("out").unwrap();
        let results = registered.get_attr("results").unwrap();
        let outputs: Vec<String> = results
            .try_iter()
            .unwrap()
            .map(|entry| entry.get_attr("output").unwrap().to_string())
            .collect();
        assert_eq!(outputs, vec!["one\n", "two\n"]);
        assert!(result.get_changed());
    }
}
