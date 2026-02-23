/// ANCHOR: module
/// # at
///
/// Schedule one-time jobs using the `at` command.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - at:
///     command: /usr/local/bin/backup.sh
///     at_time: "now + 1 hour"
///
/// - at:
///     command: /usr/local/bin/maintenance.sh
///     at_time: "23:00"
///     unique: true
///
/// - at:
///     command: /usr/local/bin/old-task.sh
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

fn default_state() -> Option<State> {
    Some(State::Present)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The command to execute.
    /// Required if state=present.
    pub command: Option<String>,
    /// Whether the job should be present or absent.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// Time to execute the command.
    /// Examples: 'now', 'now + 1 hour', '12:00', 'teatime'.
    /// Required if state=present.
    pub at_time: Option<String>,
    /// Whether to ensure only one instance of this command exists.
    /// When true, removes any existing scheduled jobs with the same command before adding.
    /// **[default: `false`]**
    #[serde(default)]
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Present,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AtJob {
    pub id: String,
    pub date: String,
    pub hour: String,
    pub command: String,
}

fn get_at_jobs() -> Result<Vec<AtJob>> {
    if let Ok(test_jobs) = std::env::var("RASH_TEST_AT_JOBS") {
        if test_jobs.is_empty() {
            return Ok(Vec::new());
        }
        return parse_atq_output(&test_jobs);
    }

    let output = Command::new("atq").output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute atq: {}", e),
        )
    })?;

    if !output.status.success() {
        if output.status.code() == Some(127) {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                "at command not found. Please install the 'at' package.",
            ));
        }
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_atq_output(&stdout)
}

fn parse_atq_output(output: &str) -> Result<Vec<AtJob>> {
    let mut jobs = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 6 {
            let job_id = parts[0].to_string();
            let date = if parts.len() >= 4 {
                format!("{} {} {}", parts[1], parts[2], parts[3])
            } else {
                String::new()
            };
            let hour = if parts.len() >= 6 {
                parts[5].to_string()
            } else {
                String::new()
            };

            let command = get_job_command(&job_id).unwrap_or_default();

            jobs.push(AtJob {
                id: job_id,
                date,
                hour,
                command,
            });
        }
    }

    Ok(jobs)
}

fn get_job_command(job_id: &str) -> Result<String> {
    if let Ok(test_cmd) = std::env::var("RASH_TEST_AT_CMD_PREFIX") {
        return Ok(test_cmd);
    }

    let output = Command::new("at")
        .arg("-c")
        .arg(job_id)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to get job details: {}", e),
            )
        })?;

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout);
        for line in content.lines().rev() {
            if !line.starts_with('#') && !line.is_empty() && !line.starts_with("cd ") {
                return Ok(line.to_string());
            }
        }
    }

    Ok(String::new())
}

fn schedule_job(command: &str, at_time: &str) -> Result<String> {
    if let Ok(_test) = std::env::var("RASH_TEST_AT_JOBS") {
        return Ok("test-job-id".to_string());
    }

    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("echo '{}' | at '{}'", command, at_time))
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to schedule job: {}", e),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("job")
            && let Some(id) = line.split_whitespace().nth(1)
        {
            return Ok(id.to_string());
        }
    }

    Ok("unknown".to_string())
}

fn remove_job(job_id: &str) -> Result<()> {
    if let Ok(_test) = std::env::var("RASH_TEST_AT_JOBS") {
        return Ok(());
    }

    let output = Command::new("atrm").arg(job_id).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove job: {}", e),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

fn find_jobs_by_command<'a>(jobs: &'a [AtJob], command: &str) -> Vec<&'a AtJob> {
    jobs.iter()
        .filter(|job| job.command.trim() == command.trim())
        .collect()
}

pub fn at(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or(State::Present);

    if state == State::Present {
        if params.command.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "command parameter is required when state=present",
            ));
        }
        if params.at_time.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "at_time parameter is required when state=present",
            ));
        }
    }

    let jobs = get_at_jobs()?;

    match state {
        State::Present => {
            let command = params.command.as_ref().unwrap();
            let at_time = params.at_time.as_ref().unwrap();

            if params.unique {
                let existing_jobs = find_jobs_by_command(&jobs, command);
                if !existing_jobs.is_empty() {
                    if check_mode {
                        return Ok(ModuleResult {
                            changed: true,
                            output: Some(format!(
                                "Would remove {} existing job(s) and add new job for: {}",
                                existing_jobs.len(),
                                command
                            )),
                            extra: None,
                        });
                    }

                    for job in existing_jobs {
                        remove_job(&job.id)?;
                    }

                    let job_id = schedule_job(command, at_time)?;
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Job {} scheduled: {}", job_id, command)),
                        extra: None,
                    });
                }
            }

            if !params.unique && !find_jobs_by_command(&jobs, command).is_empty() {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Job already scheduled: {}", command)),
                    extra: None,
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would schedule job: {} at {}", command, at_time)),
                    extra: None,
                });
            }

            let job_id = schedule_job(command, at_time)?;
            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Job {} scheduled: {}", job_id, command)),
                extra: None,
            })
        }
        State::Absent => {
            if params.command.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "command parameter is required when state=absent",
                ));
            }

            let command = params.command.as_ref().unwrap();
            let existing_jobs = find_jobs_by_command(&jobs, command);

            if existing_jobs.is_empty() {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("No job found for: {}", command)),
                    extra: None,
                });
            }

            let jobs_count = existing_jobs.len();

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!(
                        "Would remove {} job(s) for: {}",
                        jobs_count, command
                    )),
                    extra: None,
                });
            }

            for job in existing_jobs {
                remove_job(&job.id)?;
            }

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Removed {} job(s) for: {}", jobs_count, command)),
                extra: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct At;

impl Module for At {
    fn get_name(&self) -> &str {
        "at"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((at(parse_params(params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: /usr/local/bin/backup.sh
            at_time: "now + 1 hour"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Some("/usr/local/bin/backup.sh".to_string()));
        assert_eq!(params.at_time, Some("now + 1 hour".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_unique() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: /usr/local/bin/backup.sh
            at_time: "12:00"
            unique: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.unique);
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: /usr/local/bin/old-task.sh
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_at_missing_command_for_present() {
        let params = Params {
            command: None,
            state: Some(State::Present),
            at_time: Some("now".to_string()),
            unique: false,
        };
        let result = at(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("command parameter is required")
        );
    }

    #[test]
    fn test_at_missing_at_time_for_present() {
        let params = Params {
            command: Some("echo test".to_string()),
            state: Some(State::Present),
            at_time: None,
            unique: false,
        };
        let result = at(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("at_time parameter is required")
        );
    }

    #[test]
    fn test_at_missing_command_for_absent() {
        unsafe { std::env::set_var("RASH_TEST_AT_JOBS", "") };
        let params = Params {
            command: None,
            state: Some(State::Absent),
            at_time: None,
            unique: false,
        };
        let result = at(params, false);
        unsafe { std::env::remove_var("RASH_TEST_AT_JOBS") };
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("command parameter is required")
        );
    }

    #[test]
    fn test_at_present_check_mode() {
        unsafe { std::env::set_var("RASH_TEST_AT_JOBS", "") };
        let params = Params {
            command: Some("echo test".to_string()),
            state: Some(State::Present),
            at_time: Some("now".to_string()),
            unique: false,
        };
        let result = at(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would schedule job"));
        unsafe { std::env::remove_var("RASH_TEST_AT_JOBS") };
    }

    #[test]
    fn test_at_present_with_unique_check_mode() {
        unsafe { std::env::set_var("RASH_TEST_AT_JOBS", "1\tMon Feb 24 2026\t12:00\ta\troot") };
        unsafe { std::env::set_var("RASH_TEST_AT_CMD_PREFIX", "echo test") };
        let params = Params {
            command: Some("echo test".to_string()),
            state: Some(State::Present),
            at_time: Some("now + 1 hour".to_string()),
            unique: true,
        };
        let result = at(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would remove"));
        unsafe { std::env::remove_var("RASH_TEST_AT_JOBS") };
        unsafe { std::env::remove_var("RASH_TEST_AT_CMD_PREFIX") };
    }

    #[test]
    fn test_at_absent_no_jobs() {
        unsafe { std::env::set_var("RASH_TEST_AT_JOBS", "") };
        let params = Params {
            command: Some("echo nonexistent".to_string()),
            state: Some(State::Absent),
            at_time: None,
            unique: false,
        };
        let result = at(params, false).unwrap();
        assert!(!result.get_changed());
        unsafe { std::env::remove_var("RASH_TEST_AT_JOBS") };
    }

    #[test]
    fn test_parse_atq_output() {
        let output = "1\tMon Feb 24 2026\t12:00\ta\troot\n2\tTue Feb 25 2026\t15:30\ta\troot";
        let jobs = parse_atq_output(output).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "1");
        assert_eq!(jobs[0].hour, "12:00");
        assert_eq!(jobs[1].id, "2");
        assert_eq!(jobs[1].hour, "15:30");
    }

    #[test]
    fn test_find_jobs_by_command() {
        let jobs = vec![
            AtJob {
                id: "1".to_string(),
                date: "Mon Feb 24 2026".to_string(),
                hour: "12:00".to_string(),
                command: "echo hello".to_string(),
            },
            AtJob {
                id: "2".to_string(),
                date: "Tue Feb 25 2026".to_string(),
                hour: "15:30".to_string(),
                command: "echo world".to_string(),
            },
        ];
        let found = find_jobs_by_command(&jobs, "echo hello");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "1");
    }
}
