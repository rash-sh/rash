/// ANCHOR: module
/// # cronvar
///
/// Manage variables in crontab files.
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
/// - cronvar:
///     name: PATH
///     value: /usr/local/bin:/usr/bin:/bin
///
/// - cronvar:
///     name: MAILTO
///     value: admin@example.com
///     user: root
///
/// - cronvar:
///     name: SHELL
///     value: /bin/bash
///
/// - cronvar:
///     name: OLD_VAR
///     state: absent
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Variable name (e.g., SHELL, PATH, MAILTO).
    pub name: String,
    /// Variable value.
    /// Required if state=present.
    pub value: Option<String>,
    /// Whether the variable should be present or absent.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// The specific user whose crontab should be modified.
    /// Defaults to system crontab (/etc/crontab).
    pub user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Present,
}

fn get_crontab_path(user: &Option<String>) -> String {
    if let Ok(test_file) = std::env::var("RASH_TEST_CRONTAB_FILE") {
        return test_file;
    }

    if let Some(username) = user {
        format!("/var/spool/cron/crontabs/{}", username)
    } else {
        "/etc/crontab".to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CronVar {
    pub name: String,
    pub value: String,
}

impl CronVar {
    fn to_crontab_line(&self) -> String {
        format!("{}={}\n", self.name, self.value)
    }
}

fn is_env_var_name(name: &str) -> bool {
    let name_parts: Vec<&str> = name.split_whitespace().collect();
    name_parts.len() == 1
        && name_parts[0]
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_')
}

fn parse_crontab_vars(content: &str) -> Vec<CronVar> {
    let mut vars = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let name = trimmed[..eq_pos].trim();
            let value = trimmed[eq_pos + 1..].trim();

            if is_env_var_name(name) {
                vars.push(CronVar {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }
    }

    vars
}

pub fn cronvar(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or(State::Present);

    if state == State::Present && params.value.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        ));
    }

    let crontab_path = get_crontab_path(&params.user);
    let path = Path::new(&crontab_path);

    let original_content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut vars = parse_crontab_vars(&original_content);
    let existing_index = vars.iter().position(|v| v.name == params.name);

    let changed = match state {
        State::Present => {
            let value = params.value.as_ref().unwrap();
            let new_var = CronVar {
                name: params.name.clone(),
                value: value.clone(),
            };

            match existing_index {
                Some(idx) => {
                    if vars[idx] != new_var {
                        vars[idx] = new_var;
                        true
                    } else {
                        false
                    }
                }
                None => {
                    vars.push(new_var);
                    true
                }
            }
        }
        State::Absent => {
            if existing_index.is_some() {
                vars.retain(|v| v.name != params.name);
                true
            } else {
                false
            }
        }
    };

    if changed {
        let mut new_content = String::new();

        for line in original_content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                new_content.push('\n');
                continue;
            }

            let is_var_line = trimmed
                .find('=')
                .map(|eq_pos| is_env_var_name(trimmed[..eq_pos].trim()))
                .unwrap_or(false);

            if is_var_line && let Some(eq_pos) = trimmed.find('=') {
                let var_name = trimmed[..eq_pos].trim();
                if vars.iter().any(|v| v.name == var_name) {
                    continue;
                }
            }

            new_content.push_str(line);
            new_content.push('\n');
        }

        for var in &vars {
            new_content.push_str(&var.to_crontab_line());
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
pub struct Cronvar;

impl Module for Cronvar {
    fn get_name(&self) -> &str {
        "cronvar"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((cronvar(parse_params(params)?, check_mode)?, None))
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
            name: PATH
            value: /usr/local/bin:/usr/bin:/bin
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "PATH");
        assert_eq!(
            params.value,
            Some("/usr/local/bin:/usr/bin:/bin".to_string())
        );
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_user() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: MAILTO
            value: admin@example.com
            user: root
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "MAILTO");
        assert_eq!(params.user, Some("root".to_string()));
    }

    #[test]
    fn test_parse_crontab_vars() {
        let content =
            "PATH=/usr/local/bin:/usr/bin:/bin\nMAILTO=admin@example.com\nSHELL=/bin/bash\n";
        let vars = parse_crontab_vars(content);
        assert_eq!(vars.len(), 3);
        assert_eq!(vars[0].name, "PATH");
        assert_eq!(vars[0].value, "/usr/local/bin:/usr/bin:/bin");
        assert_eq!(vars[1].name, "MAILTO");
        assert_eq!(vars[1].value, "admin@example.com");
        assert_eq!(vars[2].name, "SHELL");
        assert_eq!(vars[2].value, "/bin/bash");
    }

    #[test]
    fn test_parse_crontab_vars_ignores_cron_jobs() {
        let content = "PATH=/usr/bin:/bin\n0 2 * * * root /usr/bin/backup.sh\n";
        let vars = parse_crontab_vars(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "PATH");
    }

    #[test]
    fn test_parse_crontab_vars_ignores_comments() {
        let content = "# This is a comment\nPATH=/usr/bin:/bin\n# Another comment\n";
        let vars = parse_crontab_vars(content);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "PATH");
    }

    #[test]
    fn test_cron_var_to_line() {
        let var = CronVar {
            name: "PATH".to_string(),
            value: "/usr/bin:/bin".to_string(),
        };
        assert_eq!(var.to_crontab_line(), "PATH=/usr/bin:/bin\n");
    }

    #[test]
    fn test_cronvar_missing_value_for_present() {
        let params = Params {
            name: "PATH".to_string(),
            value: None,
            state: Some(State::Present),
            user: None,
        };
        let result = cronvar(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("value parameter is required")
        );
    }

    #[test]
    fn test_get_crontab_path_user() {
        let path = get_crontab_path(&Some("testuser".to_string()));
        assert_eq!(path, "/var/spool/cron/crontabs/testuser");
    }

    #[test]
    fn test_get_crontab_path_default() {
        let path = get_crontab_path(&None);
        assert_eq!(path, "/etc/crontab");
    }

    #[test]
    fn test_cronvar_absent_no_change() {
        let params = Params {
            name: "NONEXISTENT".to_string(),
            value: None,
            state: Some(State::Absent),
            user: None,
        };
        let result = cronvar(params, false);
        assert!(result.is_ok());
        assert!(!result.unwrap().get_changed());
    }
}
