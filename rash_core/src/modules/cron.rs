/// ANCHOR: module
/// # cron
///
/// Manage cron jobs and crontab entries.
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
/// - cron:
///     name: daily-backup
///     minute: "0"
///     hour: "2"
///     job: /usr/local/bin/backup.sh
///     state: present
///
/// - cron:
///     name: weekly-cleanup
///     special_time: weekly
///     job: /usr/local/bin/cleanup.sh
///
/// - cron:
///     name: old-job
///     state: absent
///
/// - cron:
///     name: hourly-check
///     special_time: hourly
///     job: /usr/local/bin/check.sh
///     disabled: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

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

fn default_time_field() -> String {
    "*".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Description of the crontab entry.
    pub name: String,
    /// The command to execute.
    /// Required if state=present.
    pub job: Option<String>,
    /// Whether the job should be present or absent.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// Minute when the job should run (0-59, *, */2, etc).
    /// **[default: `"*"`]**
    #[serde(default = "default_time_field")]
    pub minute: String,
    /// Hour when the job should run (0-23, *, */2, etc).
    /// **[default: `"*"`]**
    #[serde(default = "default_time_field")]
    pub hour: String,
    /// Day of the month the job should run (1-31, *, */2, etc).
    /// **[default: `"*"`]**
    #[serde(default = "default_time_field")]
    pub day: String,
    /// Month of the year the job should run (1-12, JAN-DEC, *, etc).
    /// **[default: `"*"`]**
    #[serde(default = "default_time_field")]
    pub month: String,
    /// Day of the week the job should run (0-6, SUN-SAT, *, etc).
    /// **[default: `"*"`]**
    #[serde(default = "default_time_field")]
    pub weekday: String,
    /// Special time specification (hourly, daily, weekly, monthly, reboot, etc).
    /// Cannot be combined with minute, hour, day, month, weekday.
    pub special_time: Option<SpecialTime>,
    /// If the job should be disabled (commented out) in the crontab.
    /// **[default: `false`]**
    #[serde(default)]
    pub disabled: bool,
    /// The specific user whose crontab should be modified.
    /// Defaults to current user.
    pub user: Option<String>,
    /// If specified, uses this file instead of an individual user's crontab.
    /// Relative paths are interpreted with respect to /etc/cron.d.
    pub cron_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Present,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum SpecialTime {
    Annually,
    Daily,
    Hourly,
    Monthly,
    Reboot,
    Weekly,
    Yearly,
}

impl SpecialTime {
    fn to_cron_string(&self) -> &'static str {
        match self {
            SpecialTime::Annually => "@annually",
            SpecialTime::Daily => "@daily",
            SpecialTime::Hourly => "@hourly",
            SpecialTime::Monthly => "@monthly",
            SpecialTime::Reboot => "@reboot",
            SpecialTime::Weekly => "@weekly",
            SpecialTime::Yearly => "@yearly",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CronEntry {
    pub name: String,
    pub job: String,
    pub time_spec: String,
    pub disabled: bool,
}

impl CronEntry {
    fn to_crontab_line(&self) -> String {
        let disabled_prefix = if self.disabled { "#disabled#" } else { "" };
        format!(
            "# rash: {}\n{}{} {}\n",
            self.name, disabled_prefix, self.time_spec, self.job
        )
    }
}

fn get_crontab_path(user: &Option<String>, cron_file: &Option<String>) -> String {
    if let Ok(test_file) = std::env::var("RASH_TEST_CRONTAB_FILE") {
        return test_file;
    }

    if let Some(file) = cron_file {
        if Path::new(file).is_absolute() {
            file.clone()
        } else {
            format!("/etc/cron.d/{}", file)
        }
    } else if let Some(username) = user {
        format!("/var/spool/cron/crontabs/{}", username)
    } else {
        "/var/spool/cron/crontabs/root".to_string()
    }
}

fn parse_crontab(content: &str) -> Vec<CronEntry> {
    let mut entries = Vec::new();
    let mut current_name: Option<String> = None;
    let mut is_disabled = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("# rash:") || trimmed.starts_with("#Ansible:") {
            current_name = Some(
                trimmed
                    .split(':')
                    .next_back()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
            is_disabled = false;
        } else if !trimmed.starts_with('#') && !trimmed.is_empty() && current_name.is_some() {
            let disabled_prefix = "#disabled#";
            let (actual_line, disabled) =
                if let Some(stripped) = trimmed.strip_prefix(disabled_prefix) {
                    (stripped, true)
                } else {
                    (trimmed, false)
                };

            let parts: Vec<&str> = actual_line.split_whitespace().collect();
            if parts.len() >= 6 {
                let time_spec = parts[0..5].join(" ");
                let job = parts[5..].join(" ");

                entries.push(CronEntry {
                    name: current_name.clone().unwrap_or_default(),
                    job,
                    time_spec,
                    disabled: disabled || is_disabled,
                });
            } else if parts.len() == 2 {
                let time_spec = parts[0].to_string();
                let job = parts[1].to_string();

                entries.push(CronEntry {
                    name: current_name.clone().unwrap_or_default(),
                    job,
                    time_spec,
                    disabled: disabled || is_disabled,
                });
            }
            current_name = None;
        } else if trimmed.starts_with("#disabled#") {
            is_disabled = true;
        }
    }

    entries
}

fn build_time_spec(params: &Params) -> String {
    if let Some(ref special) = params.special_time {
        special.to_cron_string().to_string()
    } else {
        format!(
            "{} {} {} {} {}",
            params.minute, params.hour, params.day, params.month, params.weekday
        )
    }
}

pub fn cron(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or(State::Present);

    if state == State::Present && params.job.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "job parameter is required when state=present",
        ));
    }

    if params.special_time.is_some()
        && (params.minute != "*"
            || params.hour != "*"
            || params.day != "*"
            || params.month != "*"
            || params.weekday != "*")
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "special_time cannot be combined with minute, hour, day, month, or weekday",
        ));
    }

    let crontab_path = get_crontab_path(&params.user, &params.cron_file);
    let path = Path::new(&crontab_path);

    let original_content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut entries = parse_crontab(&original_content);
    let existing_index = entries.iter().position(|e| e.name == params.name);

    let changed = match state {
        State::Present => {
            let job = params.job.as_ref().unwrap();
            let time_spec = build_time_spec(&params);
            let new_entry = CronEntry {
                name: params.name.clone(),
                job: job.clone(),
                time_spec,
                disabled: params.disabled,
            };

            match existing_index {
                Some(idx) => {
                    if entries[idx] != new_entry {
                        entries[idx] = new_entry;
                        true
                    } else {
                        false
                    }
                }
                None => {
                    entries.push(new_entry);
                    true
                }
            }
        }
        State::Absent => {
            if existing_index.is_some() {
                entries.retain(|e| e.name != params.name);
                true
            } else {
                false
            }
        }
    };

    if changed {
        let mut new_content = String::new();

        let mut other_lines = Vec::new();
        for line in original_content.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("# rash:")
                && !trimmed.starts_with("#Ansible:")
                && !trimmed.starts_with("#disabled#")
                && !(entries.iter().any(|e| {
                    trimmed.starts_with(&e.time_spec)
                        && trimmed.contains(e.job.split_whitespace().next().unwrap_or(""))
                }))
                && !trimmed.is_empty()
            {
                other_lines.push(line);
            }
        }

        for line in &other_lines {
            new_content.push_str(line);
            new_content.push('\n');
        }

        for entry in &entries {
            new_content.push_str(&entry.to_crontab_line());
        }

        diff(&original_content, &new_content);

        if !check_mode {
            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, &new_content)?;
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(params.name),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Cron;

impl Module for Cron {
    fn get_name(&self) -> &str {
        "cron"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((cron(parse_params(params)?, check_mode)?, None))
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
            name: daily-backup
            minute: "0"
            hour: "2"
            job: /usr/local/bin/backup.sh
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "daily-backup");
        assert_eq!(params.minute, "0");
        assert_eq!(params.hour, "2");
        assert_eq!(params.job, Some("/usr/local/bin/backup.sh".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_special_time() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: weekly-cleanup
            special_time: weekly
            job: /usr/local/bin/cleanup.sh
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "weekly-cleanup");
        assert_eq!(params.special_time, Some(SpecialTime::Weekly));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_build_time_spec_standard() {
        let params = Params {
            name: "test".to_string(),
            job: Some("echo test".to_string()),
            state: Some(State::Present),
            minute: "0".to_string(),
            hour: "2".to_string(),
            day: "*".to_string(),
            month: "*".to_string(),
            weekday: "*".to_string(),
            special_time: None,
            disabled: false,
            user: None,
            cron_file: None,
        };
        assert_eq!(build_time_spec(&params), "0 2 * * *");
    }

    #[test]
    fn test_build_time_spec_special() {
        let params = Params {
            name: "test".to_string(),
            job: Some("echo test".to_string()),
            state: Some(State::Present),
            minute: "*".to_string(),
            hour: "*".to_string(),
            day: "*".to_string(),
            month: "*".to_string(),
            weekday: "*".to_string(),
            special_time: Some(SpecialTime::Hourly),
            disabled: false,
            user: None,
            cron_file: None,
        };
        assert_eq!(build_time_spec(&params), "@hourly");
    }

    #[test]
    fn test_parse_crontab() {
        let content = r#"# rash: daily-backup
0 2 * * * /usr/local/bin/backup.sh
# rash: weekly-cleanup
@weekly /usr/local/bin/cleanup.sh
"#;
        let entries = parse_crontab(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "daily-backup");
        assert_eq!(entries[0].time_spec, "0 2 * * *");
        assert_eq!(entries[0].job, "/usr/local/bin/backup.sh");
        assert_eq!(entries[1].name, "weekly-cleanup");
        assert_eq!(entries[1].time_spec, "@weekly");
        assert_eq!(entries[1].job, "/usr/local/bin/cleanup.sh");
    }

    #[test]
    fn test_cron_entry_to_line() {
        let entry = CronEntry {
            name: "test-job".to_string(),
            job: "/usr/bin/test".to_string(),
            time_spec: "0 2 * * *".to_string(),
            disabled: false,
        };
        assert_eq!(
            entry.to_crontab_line(),
            "# rash: test-job\n0 2 * * * /usr/bin/test\n"
        );
    }

    #[test]
    fn test_cron_entry_disabled() {
        let entry = CronEntry {
            name: "test-job".to_string(),
            job: "/usr/bin/test".to_string(),
            time_spec: "0 2 * * *".to_string(),
            disabled: true,
        };
        assert_eq!(
            entry.to_crontab_line(),
            "# rash: test-job\n#disabled#0 2 * * * /usr/bin/test\n"
        );
    }

    #[test]
    fn test_special_time_conversion() {
        assert_eq!(SpecialTime::Hourly.to_cron_string(), "@hourly");
        assert_eq!(SpecialTime::Daily.to_cron_string(), "@daily");
        assert_eq!(SpecialTime::Weekly.to_cron_string(), "@weekly");
        assert_eq!(SpecialTime::Monthly.to_cron_string(), "@monthly");
        assert_eq!(SpecialTime::Reboot.to_cron_string(), "@reboot");
        assert_eq!(SpecialTime::Annually.to_cron_string(), "@annually");
        assert_eq!(SpecialTime::Yearly.to_cron_string(), "@yearly");
    }

    #[test]
    fn test_cron_missing_job_for_present() {
        let params = Params {
            name: "test".to_string(),
            job: None,
            state: Some(State::Present),
            minute: "*".to_string(),
            hour: "*".to_string(),
            day: "*".to_string(),
            month: "*".to_string(),
            weekday: "*".to_string(),
            special_time: None,
            disabled: false,
            user: None,
            cron_file: None,
        };
        let result = cron(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("job parameter is required")
        );
    }

    #[test]
    fn test_cron_special_time_with_time_fields() {
        let params = Params {
            name: "test".to_string(),
            job: Some("echo test".to_string()),
            state: Some(State::Present),
            minute: "0".to_string(),
            hour: "*".to_string(),
            day: "*".to_string(),
            month: "*".to_string(),
            weekday: "*".to_string(),
            special_time: Some(SpecialTime::Hourly),
            disabled: false,
            user: None,
            cron_file: None,
        };
        let result = cron(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("special_time cannot be combined")
        );
    }

    #[test]
    fn test_get_crontab_path_user() {
        let path = get_crontab_path(&Some("testuser".to_string()), &None);
        assert_eq!(path, "/var/spool/cron/crontabs/testuser");
    }

    #[test]
    fn test_get_crontab_path_cron_file_relative() {
        let path = get_crontab_path(&None, &Some("my-cron".to_string()));
        assert_eq!(path, "/etc/cron.d/my-cron");
    }

    #[test]
    fn test_get_crontab_path_cron_file_absolute() {
        let path = get_crontab_path(&None, &Some("/opt/my-cron".to_string()));
        assert_eq!(path, "/opt/my-cron");
    }

    #[test]
    fn test_get_crontab_path_default() {
        let path = get_crontab_path(&None, &None);
        assert_eq!(path, "/var/spool/cron/crontabs/root");
    }
}
