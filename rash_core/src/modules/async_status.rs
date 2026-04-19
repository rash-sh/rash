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

fn deserialize_jid<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    
    struct JidVisitor;
    
    impl<'de> Visitor<'de> for JidVisitor {
        type Value = u64;
        
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a u64 or a string containing a u64")
        }
        
        fn visit_u64<E>(self, v: u64) -> std::result::Result<u64, E>
        where
            E: de::Error,
        {
            Ok(v)
        }
        
        fn visit_i64<E>(self, v: i64) -> std::result::Result<u64, E>
        where
            E: de::Error,
        {
            if v >= 0 {
                Ok(v as u64)
            } else {
                Err(de::Error::custom("jid must be a positive number"))
            }
        }
        
        fn visit_str<E>(self, v: &str) -> std::result::Result<u64, E>
        where
            E: de::Error,
        {
            v.parse::<u64>().map_err(|_| {
                de::Error::custom(format!("invalid jid value: '{}', expected a number", v))
            })
        }
        
        fn visit_string<E>(self, v: String) -> std::result::Result<u64, E>
        where
            E: de::Error,
        {
            self.visit_str(&v)
        }
    }
    
    deserializer.deserialize_any(JidVisitor)
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Job ID to check status for.
    #[serde(deserialize_with = "deserialize_jid")]
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
    #[serde(deserialize_with = "deserialize_jid")]
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

    #[test]
    fn test_parse_params_jid_from_string() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            jid: "123"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params, Params { jid: 123 });
    }

    #[test]
    fn test_parse_params_jid_from_string_quoted() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            jid: '456'
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params, Params { jid: 456 });
    }

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
        assert_eq!(params, PollParams { jid: 789, interval: Some(2) });
    }

    #[test]
    fn test_parse_params_jid_invalid_string() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            jid: "not_a_number"
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_jid_negative() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            jid: -1
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }
}
