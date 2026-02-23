/// ANCHOR: module
/// # at
///
/// Manage one-time scheduled jobs using the at daemon.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: partial
///   details: In check mode, the module reports what would change but does not actually schedule jobs.
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Schedule cleanup in 1 hour
///   at:
///     command: /usr/local/bin/cleanup.sh
///     at_time: now + 1 hour
///     state: present
///
/// - name: Schedule backup at specific time
///   at:
///     command: /usr/local/bin/backup.sh
///     at_time: "10:30"
///     unique: true
///
/// - name: Remove a scheduled job by name
///   at:
///     name: cleanup-task
///     state: absent
///
/// - name: Schedule command at a specific date/time
///   at:
///     command: /usr/local/bin/maintenance.sh
///     at_time: "2024-12-25 03:00"
///     name: christmas-maintenance
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

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
    /// When to execute the command (e.g., 'now + 1 hour', '10:30', 'teatime').
    /// Required if state=present.
    pub at_time: Option<String>,
    /// Whether the job should be present or absent.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// A name for this job, used for identification and removal.
    /// If not specified, a name will be generated from the command.
    pub name: Option<String>,
    /// If true, prevent duplicate jobs with the same command.
    /// **[default: `false`]**
    #[serde(default)]
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Present,
}

fn at_cmd() -> String {
    std::env::var("RASH_TEST_AT_CMD").unwrap_or_else(|_| "at".to_string())
}

fn get_atq_output() -> Result<String> {
    let atq_cmd = std::env::var("RASH_TEST_ATQ_CMD").unwrap_or_else(|_| "atq".to_string());

    let output = Command::new(&atq_cmd).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to run atq: {}", e),
        )
    })?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn get_job_details(job_id: &str) -> Result<Option<AtJob>> {
    let at_cmd = at_cmd();

    let output = Command::new(&at_cmd)
        .arg("-c")
        .arg(job_id)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to get job details: {}", e),
            )
        })?;

    if !output.status.success() {
        return Ok(None);
    }

    let content = String::from_utf8_lossy(&output.stdout).to_string();
    let lines: Vec<&str> = content.lines().collect();

    let mut name = None;
    let mut command = None;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("# rash_name:") {
            name = Some(
                trimmed
                    .strip_prefix("# rash_name:")
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        }
    }

    if let Some(last_line) = lines.last() {
        let trimmed = last_line.trim();
        if !trimmed.starts_with('#') && !trimmed.is_empty() {
            command = Some(trimmed.to_string());
        }
    }

    if let Some(cmd) = &command {
        if let Some(n) = &name {
            return Ok(Some(AtJob {
                id: job_id.to_string(),
                name: n.clone(),
                command: cmd.clone(),
            }));
        }

        let generated_name = generate_name_from_command(cmd);
        return Ok(Some(AtJob {
            id: job_id.to_string(),
            name: generated_name,
            command: cmd.clone(),
        }));
    }

    Ok(None)
}

fn generate_name_from_command(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().take(3).collect();
    parts.join("-").replace(['/', ' ', '.'], "-")
}

#[derive(Debug, Clone, PartialEq)]
pub struct AtJob {
    pub id: String,
    pub name: String,
    pub command: String,
}

fn parse_atq_output(output: &str) -> Vec<AtJob> {
    let mut jobs = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(job_id) = trimmed.split_whitespace().next()
            && let Ok(Some(job)) = get_job_details(job_id)
        {
            jobs.push(job);
        }
    }

    jobs
}

fn find_jobs_by_name<'a>(jobs: &'a [AtJob], name: &str) -> Vec<&'a AtJob> {
    jobs.iter().filter(|j| j.name == name).collect()
}

fn find_jobs_by_command<'a>(jobs: &'a [AtJob], command: &str) -> Vec<&'a AtJob> {
    jobs.iter().filter(|j| j.command == command).collect()
}

fn remove_job(job_id: &str) -> Result<()> {
    let atrm_cmd = std::env::var("RASH_TEST_ATRM_CMD").unwrap_or_else(|_| "atrm".to_string());

    let status = Command::new(&atrm_cmd).arg(job_id).status().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove job: {}", e),
        )
    })?;

    if !status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove job {}", job_id),
        ));
    }

    Ok(())
}

fn schedule_job(command: &str, at_time: &str, name: &str) -> Result<String> {
    let at_cmd = at_cmd();

    let script_content = format!("# rash_name: {}\n{}", name, command);

    let mut child = Command::new(&at_cmd)
        .arg(at_time)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to run at: {}", e),
            )
        })?;

    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(script_content.as_bytes()).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to write to at stdin: {}", e),
            )
        })?;
    }

    let output = child.wait_with_output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to wait for at: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("at command failed: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("job ")
            && let Some(job_id) = line.split_whitespace().nth(1)
        {
            return Ok(job_id.to_string());
        }
    }

    Ok(String::new())
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

    let job_name = params.name.clone().unwrap_or_else(|| {
        if let Some(ref cmd) = params.command {
            generate_name_from_command(cmd)
        } else {
            "unnamed-job".to_string()
        }
    });

    let atq_output = get_atq_output()?;
    let existing_jobs = parse_atq_output(&atq_output);

    match state {
        State::Present => {
            let command = params.command.as_ref().unwrap();
            let at_time = params.at_time.as_ref().unwrap();

            if params.unique {
                let duplicates = find_jobs_by_command(&existing_jobs, command);
                if !duplicates.is_empty() {
                    return Ok(ModuleResult {
                        changed: false,
                        output: Some(format!("Job already scheduled (id: {})", duplicates[0].id)),
                        extra: None,
                    });
                }
            }

            let name_matches = find_jobs_by_name(&existing_jobs, &job_name);

            if !name_matches.is_empty() {
                let matching = name_matches.iter().find(|j| j.command == *command);
                if let Some(_job) = matching {
                    return Ok(ModuleResult {
                        changed: false,
                        output: Some(format!(
                            "Job '{}' already exists with same command",
                            job_name
                        )),
                        extra: None,
                    });
                }

                if !check_mode {
                    for job in name_matches {
                        remove_job(&job.id)?;
                    }
                }

                diff(
                    format!("Removing old job: {}", job_name),
                    format!("Scheduling new job: {}", job_name),
                );

                if !check_mode {
                    let job_id = schedule_job(command, at_time, &job_name)?;
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Job '{}' updated (id: {})", job_name, job_id)),
                        extra: None,
                    });
                }

                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Job '{}' would be updated", job_name)),
                    extra: None,
                });
            }

            diff(
                "",
                format!("Scheduling job '{}' at '{}'", job_name, at_time),
            );

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Job '{}' would be scheduled", job_name)),
                    extra: None,
                });
            }

            let job_id = schedule_job(command, at_time, &job_name)?;

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Job '{}' scheduled (id: {})", job_name, job_id)),
                extra: None,
            })
        }
        State::Absent => {
            let name_matches = find_jobs_by_name(&existing_jobs, &job_name);

            if name_matches.is_empty() {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Job '{}' not found", job_name)),
                    extra: None,
                });
            }

            diff(
                format!("Jobs matching '{}': {}", job_name, name_matches.len()),
                "",
            );

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Job '{}' would be removed", job_name)),
                    extra: None,
                });
            }

            for job in name_matches {
                remove_job(&job.id)?;
            }

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Job '{}' removed", job_name)),
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
            command: /usr/local/bin/cleanup.sh
            at_time: "now + 1 hour"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.command,
            Some("/usr/local/bin/cleanup.sh".to_string())
        );
        assert_eq!(params.at_time, Some("now + 1 hour".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: cleanup-task
            command: /usr/local/bin/cleanup.sh
            at_time: "10:30"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("cleanup-task".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_unique() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: /usr/local/bin/cleanup.sh
            at_time: "now + 1 hour"
            unique: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.unique);
    }

    #[test]
    fn test_generate_name_from_command() {
        assert_eq!(
            generate_name_from_command("/usr/local/bin/cleanup.sh"),
            "-usr-local-bin-cleanup-sh"
        );
        assert_eq!(
            generate_name_from_command("echo hello world"),
            "echo-hello-world"
        );
    }

    #[test]
    fn test_at_missing_command_for_present() {
        let params = Params {
            command: None,
            at_time: Some("now + 1 hour".to_string()),
            state: Some(State::Present),
            name: None,
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
            command: Some("/usr/local/bin/cleanup.sh".to_string()),
            at_time: None,
            state: Some(State::Present),
            name: None,
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
}
