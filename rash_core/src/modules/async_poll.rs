/// ANCHOR: module
/// # async_poll
///
/// Wait until an async task finishes and return its result. The task fails if the job failed.
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
/// - name: Wait for it
///   async_poll:
///     jid: "{{ job.rash_job_id }}"
///     interval: 5
///   register: result
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::job::{JobStatus, get_job_info, job_exists};
use crate::modules::async_status::deserialize_jid;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::thread;
use std::time::Duration;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug)]
pub struct AsyncPoll;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PollParams {
    /// Job ID to poll.
    #[serde(deserialize_with = "deserialize_jid")]
    pub jid: u64,
    /// Poll interval in seconds.
    pub interval: Option<u64>,
}

impl Module for AsyncPoll {
    fn get_name(&self) -> &str {
        "async_poll"
    }

    fn is_control_flow(&self) -> bool {
        true
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
                    thread::sleep(Duration::from_secs(interval));
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
    fn test_parse_poll_params_jid_from_string() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            jid: "789"
            interval: 2
            "#,
        )
        .unwrap();
        let params: PollParams = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            PollParams {
                jid: 789,
                interval: Some(2)
            }
        );
    }
}
