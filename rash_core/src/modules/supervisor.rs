/// ANCHOR: module
/// # supervisor
///
/// Manage Supervisor process control daemon programs.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Start myapp program
///   supervisor:
///     name: myapp
///     state: started
///
/// - name: Stop myapp program
///   supervisor:
///     name: myapp
///     state: stopped
///
/// - name: Restart myapp program
///   supervisor:
///     name: myapp
///     state: restarted
///
/// - name: Enable myapp program with command
///   supervisor:
///     name: myapp
///     command: /usr/bin/myapp --port 8080
///     state: started
///     enabled: true
///     user: appuser
///     autostart: true
///     autorestart: true
///     stdout_logfile: /var/log/myapp stdout.log
///     stderr_logfile: /var/log/myapp stderr.log
///
/// - name: Disable myapp program
///   supervisor:
///     name: myapp
///     enabled: false
///
/// - name: Start myapp with environment variables
///   supervisor:
///     name: myapp
///     command: /usr/bin/myapp
///     state: started
///     environment:
///       PORT: "8080"
///       NODE_ENV: production
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const SUPERVISORD_CONF_DIR: &str = "/etc/supervisor/conf.d";
#[allow(dead_code)]
const SUPERVISORD_CONF_FILE: &str = "/etc/supervisord.conf";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Reloaded,
    Restarted,
    Started,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
enum AutoRestart {
    True,
    False,
    Unexpected,
}

impl AutoRestart {
    fn to_config_value(&self) -> &str {
        match self {
            AutoRestart::True => "true",
            AutoRestart::False => "false",
            AutoRestart::Unexpected => "unexpected",
        }
    }
}

impl<'de> Deserialize<'de> for AutoRestart {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = YamlValue::deserialize(deserializer)?;
        match value {
            YamlValue::Bool(true) => Ok(AutoRestart::True),
            YamlValue::Bool(false) => Ok(AutoRestart::False),
            YamlValue::String(s) => match s.to_lowercase().as_str() {
                "true" => Ok(AutoRestart::True),
                "false" => Ok(AutoRestart::False),
                "unexpected" => Ok(AutoRestart::Unexpected),
                _ => Err(serde::de::Error::custom(format!(
                    "invalid autorestart value: {}",
                    s
                ))),
            },
            _ => Err(serde::de::Error::custom(
                "autorestart must be true, false, or unexpected",
            )),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Program name to manage.
    name: String,
    /// Command to run for the program.
    command: Option<String>,
    /// State of the program.
    state: Option<State>,
    /// Whether the program should be enabled (have a config file) or not.
    enabled: Option<bool>,
    /// User to run the program as.
    user: Option<String>,
    /// Whether the program should auto-start with supervisord.
    autostart: Option<bool>,
    /// Whether the program should auto-restart on exit.
    autorestart: Option<AutoRestart>,
    /// Path to stdout log file.
    stdout_logfile: Option<String>,
    /// Path to stderr log file.
    stderr_logfile: Option<String>,
    /// Environment variables for the program.
    environment: Option<HashMap<String, String>>,
    /// Path to supervisor config directory.
    config_dir: Option<String>,
}

#[derive(Debug)]
pub struct Supervisor;

impl Module for Supervisor {
    fn get_name(&self) -> &str {
        "supervisor"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            supervisor(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        true
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct SupervisorctlClient {
    check_mode: bool,
}

impl SupervisorctlClient {
    fn new(check_mode: bool) -> Self {
        SupervisorctlClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("supervisorctl")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `supervisorctl {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing supervisorctl: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn execute_command_with_output(&self, args: &[&str]) -> Result<SupervisorResult> {
        if self.check_mode {
            return Ok(SupervisorResult::new(true, None));
        }

        let output = self.exec_cmd(args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(SupervisorResult::new(true, output_str))
    }

    fn is_running(&self, program: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(false);
        }
        let output = self.exec_cmd(&["status", program], false)?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout.contains("RUNNING"))
    }

    fn status(&self, program: &str) -> Result<String> {
        if self.check_mode {
            return Ok(format!("{} STOPPED", program));
        }
        let output = self.exec_cmd(&["status", program], false)?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn start(&self, program: &str) -> Result<SupervisorResult> {
        let is_currently_running = self.is_running(program)?;
        if is_currently_running {
            return Ok(SupervisorResult::no_change());
        }
        self.execute_command_with_output(&["start", program])
    }

    fn stop(&self, program: &str) -> Result<SupervisorResult> {
        let is_currently_running = self.is_running(program)?;
        if !is_currently_running {
            return Ok(SupervisorResult::no_change());
        }
        self.execute_command_with_output(&["stop", program])
    }

    fn restart(&self, program: &str) -> Result<SupervisorResult> {
        self.execute_command_with_output(&["restart", program])
    }

    fn reread_and_update(&self) -> Result<SupervisorResult> {
        if self.check_mode {
            return Ok(SupervisorResult::new(true, None));
        }

        let reread_output = self.exec_cmd(&["reread"], true)?;
        let reread_stdout = String::from_utf8_lossy(&reread_output.stdout).to_string();

        if reread_stdout
            .trim()
            .contains("No config updates to process")
        {
            return Ok(SupervisorResult::no_change());
        }

        let update_output = self.exec_cmd(&["update"], true)?;
        let update_stdout = String::from_utf8_lossy(&update_output.stdout);
        let output_str = if update_stdout.trim().is_empty() {
            None
        } else {
            Some(update_stdout.trim().to_string())
        };
        Ok(SupervisorResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct SupervisorResult {
    changed: bool,
    output: Option<String>,
}

impl SupervisorResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        SupervisorResult { changed, output }
    }

    fn no_change() -> Self {
        SupervisorResult {
            changed: false,
            output: None,
        }
    }
}

fn get_config_dir(params: &Params) -> String {
    params
        .config_dir
        .clone()
        .unwrap_or_else(|| SUPERVISORD_CONF_DIR.to_string())
}

fn get_config_path(params: &Params) -> String {
    let config_dir = get_config_dir(params);
    format!("{}/{}.conf", config_dir, params.name)
}

#[allow(dead_code)]
fn detect_config_dir() -> String {
    if Path::new(SUPERVISORD_CONF_DIR).exists() {
        SUPERVISORD_CONF_DIR.to_string()
    } else {
        SUPERVISORD_CONF_FILE.to_string()
    }
}

fn generate_config_content(params: &Params) -> String {
    let mut lines = Vec::new();
    lines.push(format!("[program:{}]", params.name));

    if let Some(ref command) = params.command {
        lines.push(format!("command={}", command));
    }

    if let Some(ref user) = params.user {
        lines.push(format!("user={}", user));
    }

    if let Some(autostart) = params.autostart {
        lines.push(format!("autostart={}", autostart));
    }

    if let Some(ref autorestart) = params.autorestart {
        lines.push(format!("autorestart={}", autorestart.to_config_value()));
    }

    if let Some(ref stdout_logfile) = params.stdout_logfile {
        lines.push(format!("stdout_logfile={}", stdout_logfile));
    }

    if let Some(ref stderr_logfile) = params.stderr_logfile {
        lines.push(format!("stderr_logfile={}", stderr_logfile));
    }

    if let Some(ref env) = params.environment {
        let env_pairs: Vec<String> = env
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v.replace('"', "\\\"")))
            .collect();
        lines.push(format!("environment={}", env_pairs.join(",")));
    }

    lines.join("\n") + "\n"
}

fn validate_program_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Program name cannot be empty",
        ));
    }

    if name.len() > 255 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Program name too long (max 255 characters)",
        ));
    }

    if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains(' ') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Program name contains invalid characters",
        ));
    }

    if name.chars().any(|c| c.is_control()) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Program name contains control characters",
        ));
    }

    Ok(())
}

fn is_config_enabled(params: &Params) -> bool {
    let config_path = get_config_path(params);
    Path::new(&config_path).exists()
}

fn would_config_change(params: &Params) -> bool {
    let config_path = get_config_path(params);
    if !Path::new(&config_path).exists() {
        return true;
    }
    let content = generate_config_content(params);
    match fs::read_to_string(&config_path) {
        Ok(existing) => existing != content,
        Err(_) => true,
    }
}

fn write_config(params: &Params) -> Result<bool> {
    let config_path = get_config_path(params);
    let config_dir = get_config_dir(params);
    let content = generate_config_content(params);

    let dir = Path::new(&config_dir);
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create config directory '{}': {}", config_dir, e),
            )
        })?;
    }

    let existing_content = if Path::new(&config_path).exists() {
        fs::read_to_string(&config_path).ok()
    } else {
        None
    };

    if existing_content.as_deref() == Some(content.as_str()) {
        return Ok(false);
    }

    fs::write(&config_path, &content).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to write config file '{}': {}", config_path, e),
        )
    })?;

    Ok(true)
}

fn remove_config(params: &Params) -> Result<bool> {
    let config_path = get_config_path(params);
    if Path::new(&config_path).exists() {
        fs::remove_file(&config_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to remove config file '{}': {}", config_path, e),
            )
        })?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn supervisor(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_program_name(&params.name)?;

    let client = SupervisorctlClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    if let Some(should_be_enabled) = params.enabled {
        if should_be_enabled {
            if params.command.is_none() && !is_config_enabled(&params) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "command is required when enabling a new program",
                ));
            }

            if params.command.is_some() {
                if !check_mode {
                    let config_changed = write_config(&params)?;
                    if config_changed {
                        diff("enabled: false".to_string(), "enabled: true".to_string());
                        output_messages
                            .push(format!("Config written for program '{}'", params.name));
                        changed = true;

                        let reload_result = client.reread_and_update()?;
                        if reload_result.changed
                            && let Some(output) = reload_result.output
                        {
                            output_messages.push(output);
                        }
                    }
                } else if would_config_change(&params) {
                    diff("enabled: false".to_string(), "enabled: true".to_string());
                    output_messages
                        .push(format!("Would write config for program '{}'", params.name));
                    changed = true;
                }
            } else if !is_config_enabled(&params) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Program '{}' has no config file and no command provided",
                        params.name
                    ),
                ));
            }
        } else if is_config_enabled(&params) {
            if !check_mode {
                let was_running = client.is_running(&params.name).unwrap_or(false);

                if was_running {
                    let stop_result = client.stop(&params.name)?;
                    if let Some(output) = stop_result.output {
                        output_messages.push(output);
                    }
                }

                remove_config(&params)?;
                diff("enabled: true".to_string(), "enabled: false".to_string());
                output_messages.push(format!("Config removed for program '{}'", params.name));
                changed = true;

                let reload_result = client.reread_and_update()?;
                if reload_result.changed
                    && let Some(output) = reload_result.output
                {
                    output_messages.push(output);
                }
            } else {
                diff("enabled: true".to_string(), "enabled: false".to_string());
                output_messages.push(format!("Would remove config for program '{}'", params.name));
                changed = true;
            }
        }
    }

    if let Some(ref state) = params.state {
        match state {
            State::Started => {
                let start_result = client.start(&params.name)?;
                if start_result.changed {
                    diff("state: stopped".to_string(), "state: started".to_string());
                    if let Some(output) = start_result.output {
                        output_messages.push(output);
                    }
                }
                changed |= start_result.changed;
            }
            State::Stopped => {
                let stop_result = client.stop(&params.name)?;
                if stop_result.changed {
                    diff("state: started".to_string(), "state: stopped".to_string());
                    if let Some(output) = stop_result.output {
                        output_messages.push(output);
                    }
                }
                changed |= stop_result.changed;
            }
            State::Restarted => {
                let restart_result = client.restart(&params.name)?;
                if let Some(output) = restart_result.output {
                    output_messages.push(output);
                }
                changed |= restart_result.changed;
            }
            State::Reloaded => {
                let reload_result = client.reread_and_update()?;
                if reload_result.changed
                    && let Some(output) = reload_result.output
                {
                    output_messages.push(output);
                }
                changed |= reload_result.changed;
            }
        }
    }

    let mut extra = serde_json::Map::new();
    extra.insert(
        "name".to_string(),
        serde_json::Value::String(params.name.clone()),
    );

    if !check_mode {
        let status_output = client.status(&params.name)?;
        let is_running = status_output.contains("RUNNING");
        extra.insert("running".to_string(), serde_json::Value::Bool(is_running));
        extra.insert(
            "status".to_string(),
            serde_json::Value::String(status_output),
        );
    }

    extra.insert(
        "enabled".to_string(),
        serde_json::Value::Bool(is_config_enabled(&params)),
    );

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult {
        changed,
        output: final_output,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            command: /usr/bin/myapp
            state: started
            enabled: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.command, Some("/usr/bin/myapp".to_string()));
        assert_eq!(params.state, Some(State::Started));
        assert_eq!(params.enabled, Some(true));
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            command: /usr/bin/myapp --port 8080
            state: started
            enabled: true
            user: appuser
            autostart: true
            autorestart: true
            stdout_logfile: /var/log/myapp stdout.log
            stderr_logfile: /var/log/myapp stderr.log
            environment:
              PORT: "8080"
              NODE_ENV: production
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(
            params.command,
            Some("/usr/bin/myapp --port 8080".to_string())
        );
        assert_eq!(params.user, Some("appuser".to_string()));
        assert_eq!(params.autostart, Some(true));
        assert_eq!(params.autorestart, Some(AutoRestart::True));
        assert_eq!(
            params.stdout_logfile,
            Some("/var/log/myapp stdout.log".to_string())
        );
        assert_eq!(
            params.stderr_logfile,
            Some("/var/log/myapp stderr.log".to_string())
        );
        let env = params.environment.unwrap();
        assert_eq!(env.get("PORT").unwrap(), "8080");
        assert_eq!(env.get("NODE_ENV").unwrap(), "production");
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.command, None);
        assert_eq!(params.state, None);
        assert_eq!(params.enabled, None);
        assert_eq!(params.user, None);
        assert_eq!(params.autostart, None);
        assert_eq!(params.autorestart, None);
    }

    #[test]
    fn test_parse_params_states() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: stopped
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Stopped));

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: restarted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Restarted));

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: reloaded
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Reloaded));
    }

    #[test]
    fn test_parse_params_autorestart_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            autorestart: unexpected
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.autorestart, Some(AutoRestart::Unexpected));

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            autorestart: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.autorestart, Some(AutoRestart::False));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_program_name() {
        assert!(validate_program_name("myapp").is_ok());
        assert!(validate_program_name("my-app").is_ok());
        assert!(validate_program_name("my_app").is_ok());
        assert!(validate_program_name("my.app").is_ok());

        assert!(validate_program_name("").is_err());
        assert!(validate_program_name("a".repeat(256).as_str()).is_err());
        assert!(validate_program_name("invalid/name").is_err());
        assert!(validate_program_name("invalid\\name").is_err());
        assert!(validate_program_name("invalid\0name").is_err());
        assert!(validate_program_name("invalid name").is_err());
        assert!(validate_program_name("invalid\x1Fname").is_err());
    }

    #[test]
    fn test_generate_config_content() {
        let params = Params {
            name: "myapp".to_string(),
            command: Some("/usr/bin/myapp".to_string()),
            state: None,
            enabled: None,
            user: Some("appuser".to_string()),
            autostart: Some(true),
            autorestart: Some(AutoRestart::True),
            stdout_logfile: Some("/var/log/myapp.log".to_string()),
            stderr_logfile: None,
            environment: None,
            config_dir: None,
        };

        let content = generate_config_content(&params);
        assert!(content.contains("[program:myapp]"));
        assert!(content.contains("command=/usr/bin/myapp"));
        assert!(content.contains("user=appuser"));
        assert!(content.contains("autostart=true"));
        assert!(content.contains("autorestart=true"));
        assert!(content.contains("stdout_logfile=/var/log/myapp.log"));
        assert!(!content.contains("stderr_logfile"));
    }

    #[test]
    fn test_generate_config_content_with_environment() {
        let mut env = HashMap::new();
        env.insert("PORT".to_string(), "8080".to_string());
        env.insert("NODE_ENV".to_string(), "production".to_string());

        let params = Params {
            name: "myapp".to_string(),
            command: Some("/usr/bin/myapp".to_string()),
            state: None,
            enabled: None,
            user: None,
            autostart: None,
            autorestart: None,
            stdout_logfile: None,
            stderr_logfile: None,
            environment: Some(env),
            config_dir: None,
        };

        let content = generate_config_content(&params);
        assert!(content.contains("environment="));
        assert!(content.contains("PORT=\"8080\""));
        assert!(content.contains("NODE_ENV=\"production\""));
    }

    #[test]
    fn test_generate_config_content_autorestart_unexpected() {
        let params = Params {
            name: "myapp".to_string(),
            command: Some("/usr/bin/myapp".to_string()),
            state: None,
            enabled: None,
            user: None,
            autostart: None,
            autorestart: Some(AutoRestart::Unexpected),
            stdout_logfile: None,
            stderr_logfile: None,
            environment: None,
            config_dir: None,
        };

        let content = generate_config_content(&params);
        assert!(content.contains("autorestart=unexpected"));
    }

    #[test]
    fn test_generate_config_minimal() {
        let params = Params {
            name: "myapp".to_string(),
            command: None,
            state: None,
            enabled: None,
            user: None,
            autostart: None,
            autorestart: None,
            stdout_logfile: None,
            stderr_logfile: None,
            environment: None,
            config_dir: None,
        };

        let content = generate_config_content(&params);
        assert!(content.contains("[program:myapp]"));
        assert_eq!(content.lines().count(), 1);
    }

    #[test]
    fn test_get_config_path() {
        let params = Params {
            name: "myapp".to_string(),
            command: None,
            state: None,
            enabled: None,
            user: None,
            autostart: None,
            autorestart: None,
            stdout_logfile: None,
            stderr_logfile: None,
            environment: None,
            config_dir: None,
        };
        assert_eq!(
            get_config_path(&params),
            "/etc/supervisor/conf.d/myapp.conf"
        );

        let params_custom = Params {
            name: "myapp".to_string(),
            command: None,
            state: None,
            enabled: None,
            user: None,
            autostart: None,
            autorestart: None,
            stdout_logfile: None,
            stderr_logfile: None,
            environment: None,
            config_dir: Some("/etc/custom/supervisor".to_string()),
        };
        assert_eq!(
            get_config_path(&params_custom),
            "/etc/custom/supervisor/myapp.conf"
        );
    }

    #[test]
    fn test_auto_restart_to_config_value() {
        assert_eq!(AutoRestart::True.to_config_value(), "true");
        assert_eq!(AutoRestart::False.to_config_value(), "false");
        assert_eq!(AutoRestart::Unexpected.to_config_value(), "unexpected");
    }

    #[test]
    fn test_detect_config_dir() {
        let dir = detect_config_dir();
        assert!(!dir.is_empty());
    }

    #[test]
    fn test_parse_params_config_dir() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            command: /usr/bin/myapp
            config_dir: /etc/custom/supervisor
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.config_dir,
            Some("/etc/custom/supervisor".to_string())
        );
    }

    #[test]
    fn test_would_config_change_no_existing_file() {
        let params = Params {
            name: "nonexistent_test_program_12345".to_string(),
            command: Some("/usr/bin/test".to_string()),
            state: None,
            enabled: None,
            user: None,
            autostart: None,
            autorestart: None,
            stdout_logfile: None,
            stderr_logfile: None,
            environment: None,
            config_dir: None,
        };
        assert!(would_config_change(&params));
    }

    #[test]
    fn test_generate_config_content_escapes_quotes_in_env() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar\"baz".to_string());

        let params = Params {
            name: "myapp".to_string(),
            command: Some("/usr/bin/myapp".to_string()),
            state: None,
            enabled: None,
            user: None,
            autostart: None,
            autorestart: None,
            stdout_logfile: None,
            stderr_logfile: None,
            environment: Some(env),
            config_dir: None,
        };

        let content = generate_config_content(&params);
        assert!(content.contains("FOO=\"bar\\\"baz\""));
        assert!(!content.contains("FOO=\"bar\"baz\""));
    }
}
