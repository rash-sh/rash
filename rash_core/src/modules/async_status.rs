/// ANCHOR: module
/// # async_status
///
/// Check the status of an async task.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Start background task
///   command: ./long_running.sh
///   async: 300
///   poll: 0
///   register: job
///
/// - name: Check job status
///   async_status:
///     jid: "{{ job.rash_job_id }}"
///   register: result
///   until: result.finished
///   retries: 30
///   delay: 10
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::job::{JobStatus, get_job_info, job_exists};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Job ID to check status for.
    pub jid: u64,
}

#[derive(Debug)]
pub struct AsyncStatus;

impl Module for AsyncStatus {
    fn get_name(&self) -> &str {
        "async_status"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;

        if !job_exists(params.jid) {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Job with ID {} not found", params.jid),
            ));
        }

        let info = get_job_info(params.jid).ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("Job with ID {} not found", params.jid),
            )
        })?;

        let finished = matches!(info.status, JobStatus::Finished | JobStatus::Failed);
        let failed = matches!(info.status, JobStatus::Failed);

        let extra = Some(value::to_value(json!({
            "jid": params.jid,
            "status": serde_json::to_string(&info.status).unwrap_or_default(),
            "finished": finished,
            "failed": failed,
            "output": info.output,
            "error": info.error,
            "changed": info.changed,
            "elapsed": info.elapsed.as_secs(),
        }))?);

        let output_str = match &info.status {
            JobStatus::Running => format!(
                "Job {} still running ({}s elapsed)",
                params.jid,
                info.elapsed.as_secs()
            ),
            JobStatus::Finished => format!("Job {} finished", params.jid),
            JobStatus::Failed => {
                format!(
                    "Job {} failed: {}",
                    params.jid,
                    info.error.unwrap_or_default()
                )
            }
            JobStatus::Pending => format!("Job {} pending", params.jid),
        };

        Ok((
            ModuleResult::new(info.changed, extra, Some(output_str)),
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[derive(Debug)]
pub struct AsyncPoll;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PollParams {
    /// Job ID to poll.
    pub jid: u64,
    /// Poll interval in seconds.
    pub interval: Option<u64>,
}

impl Module for AsyncPoll {
    fn get_name(&self) -> &str {
        "async_poll"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: PollParams = parse_params(optional_params)?;

        if !job_exists(params.jid) {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Job with ID {} not found", params.jid),
            ));
        }

        let interval = params.interval.unwrap_or(1);
        let _sleep_duration = std::time::Duration::from_secs(interval);

        loop {
            let info = get_job_info(params.jid).ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("Job with ID {} not found", params.jid),
                )
            })?;

            match info.status {
                JobStatus::Finished => {
                    let extra = Some(value::to_value(json!({
                        "jid": params.jid,
                        "status": "finished",
                        "finished": true,
                        "failed": false,
                        "output": info.output,
                        "changed": info.changed,
                        "elapsed": info.elapsed.as_secs(),
                    }))?);
                    return Ok((ModuleResult::new(info.changed, extra, info.output), None));
                }
                JobStatus::Failed => {
                    let extra = Some(value::to_value(json!({
                        "jid": params.jid,
                        "status": "failed",
                        "finished": true,
                        "failed": true,
                        "output": info.output,
                        "error": info.error,
                        "changed": info.changed,
                        "elapsed": info.elapsed.as_secs(),
                    }))?);
                    return Ok((ModuleResult::new(info.changed, extra, info.output), None));
                }
                JobStatus::Running | JobStatus::Pending => {
                    trace!(
                        "Job {} still running, sleeping for {}s",
                        params.jid, interval
                    );
                    process::Command::new("sleep")
                        .arg(interval.to_string())
                        .output()
                        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
                }
            }
        }
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(PollParams::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            jid: 123
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params, Params { jid: 123 });
    }
}
