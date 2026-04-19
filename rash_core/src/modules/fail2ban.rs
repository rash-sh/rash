/// ANCHOR: module
/// # fail2ban
///
/// Manage Fail2ban intrusion prevention system.
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
/// - name: Create SSH jail
///   fail2ban:
///     name: sshd
///     state: present
///     enabled: true
///     port: ssh
///     filter: sshd
///     logpath: /var/log/auth.log
///     maxretry: 5
///     findtime: 600
///     bantime: 3600
///
/// - name: Create nginx HTTP auth jail
///   fail2ban:
///     name: nginx-http-auth
///     state: present
///     enabled: true
///     port: http,https
///     filter: nginx-http-auth
///     logpath: /var/log/nginx/error.log
///     maxretry: 3
///
/// - name: Disable a jail
///   fail2ban:
///     name: sshd
///     enabled: false
///
/// - name: Remove a jail
///   fail2ban:
///     name: sshd
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Write;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

const JAIL_CONF_DIR: &str = "/etc/fail2ban/jail.d";
const JAIL_CONF_SUFFIX: &str = ".local";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Jail name (required).
    pub name: String,
    /// Whether the jail should be present or absent.
    /// **[default: `present`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// Whether the jail should be enabled or disabled.
    /// **[default: `true`]**
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Port(s) to protect (e.g., ssh, http, https, or 22, 80, 443).
    pub port: Option<String>,
    /// Filter name to use for this jail.
    pub filter: Option<String>,
    /// Log file path to monitor.
    pub logpath: Option<String>,
    /// Maximum number of retries before ban.
    /// **[default: 5]**
    pub maxretry: Option<u32>,
    /// Time window in seconds for counting retries.
    /// **[default: 600]**
    pub findtime: Option<u64>,
    /// Ban duration in seconds.
    /// **[default: 600]**
    pub bantime: Option<u64>,
    /// Action to take on ban (e.g., `%(action_)s`, `%(action_mwl)s`).
    pub action: Option<String>,
}

fn default_state() -> State {
    State::Present
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

#[derive(Debug)]
pub struct Fail2ban;

impl Module for Fail2ban {
    fn get_name(&self) -> &str {
        "fail2ban"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((fail2ban(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn get_jail_config_path(name: &str) -> String {
    format!("{}/{}{}", JAIL_CONF_DIR, name, JAIL_CONF_SUFFIX)
}

fn validate_jail_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Jail name cannot be empty",
        ));
    }

    if name.len() > 255 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Jail name too long (max 255 characters)",
        ));
    }

    let valid_chars = name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_');

    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Jail name can only contain alphanumeric characters, hyphens, and underscores",
        ));
    }

    Ok(())
}

fn jail_exists(name: &str) -> bool {
    let path = get_jail_config_path(name);
    Path::new(&path).exists()
}

fn generate_jail_config(params: &Params) -> String {
    let mut config = format!("[{}]\n", params.name);

    config.push_str(&format!("enabled = {}\n", params.enabled));

    if let Some(port) = &params.port {
        config.push_str(&format!("port = {}\n", port));
    }

    if let Some(filter) = &params.filter {
        config.push_str(&format!("filter = {}\n", filter));
    }

    if let Some(logpath) = &params.logpath {
        config.push_str(&format!("logpath = {}\n", logpath));
    }

    if let Some(maxretry) = params.maxretry {
        config.push_str(&format!("maxretry = {}\n", maxretry));
    }

    if let Some(findtime) = params.findtime {
        config.push_str(&format!("findtime = {}\n", findtime));
    }

    if let Some(bantime) = params.bantime {
        config.push_str(&format!("bantime = {}\n", bantime));
    }

    if let Some(action) = &params.action {
        config.push_str(&format!("action = {}\n", action));
    }

    config
}

fn create_jail(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let config_path = get_jail_config_path(&params.name);

    if jail_exists(&params.name) {
        if check_mode {
            return Ok(ModuleResult::new(
                false,
                None,
                Some(format!("Jail '{}' would be updated", params.name)),
            ));
        }

        let new_config = generate_jail_config(params);
        let existing_config = fs::read_to_string(&config_path).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to read jail config: {e}"),
            )
        })?;

        if existing_config.trim() == new_config.trim() {
            return Ok(ModuleResult::new(
                false,
                None,
                Some(format!(
                    "Jail '{}' already configured correctly",
                    params.name
                )),
            ));
        }

        fs::write(&config_path, &new_config).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to write jail config: {e}"),
            )
        })?;

        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Jail '{}' updated", params.name)),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Jail '{}' would be created", params.name)),
        ));
    }

    fs::create_dir_all(JAIL_CONF_DIR).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create jail config directory: {e}"),
        )
    })?;

    let config = generate_jail_config(params);
    let mut file = fs::File::create(&config_path).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create jail config file: {e}"),
        )
    })?;

    file.write_all(config.as_bytes()).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to write jail config: {e}"),
        )
    })?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Jail '{}' created", params.name)),
    ))
}

fn remove_jail(name: &str, check_mode: bool) -> Result<ModuleResult> {
    if !jail_exists(name) {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!("Jail '{}' does not exist", name)),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Jail '{}' would be removed", name)),
        ));
    }

    let config_path = get_jail_config_path(name);
    fs::remove_file(&config_path).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove jail config: {e}"),
        )
    })?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Jail '{}' removed", name)),
    ))
}

fn fail2ban(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    validate_jail_name(&params.name)?;

    match params.state {
        State::Present => create_jail(&params, check_mode),
        State::Absent => remove_jail(&params.name, check_mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: sshd
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "sshd");
        assert_eq!(params.state, State::Present);
        assert!(params.enabled);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: sshd
            state: present
            enabled: true
            port: ssh
            filter: sshd
            logpath: /var/log/auth.log
            maxretry: 5
            findtime: 600
            bantime: 3600
            action: "%(action_mwl)s"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "sshd");
        assert_eq!(params.state, State::Present);
        assert!(params.enabled);
        assert_eq!(params.port, Some("ssh".to_owned()));
        assert_eq!(params.filter, Some("sshd".to_owned()));
        assert_eq!(params.logpath, Some("/var/log/auth.log".to_owned()));
        assert_eq!(params.maxretry, Some(5));
        assert_eq!(params.findtime, Some(600));
        assert_eq!(params.bantime, Some(3600));
        assert_eq!(params.action, Some("%(action_mwl)s".to_owned()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: sshd
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "sshd");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_disabled() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: sshd
            enabled: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "sshd");
        assert!(!params.enabled);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: sshd
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_jail_name() {
        assert!(validate_jail_name("sshd").is_ok());
        assert!(validate_jail_name("nginx-http-auth").is_ok());
        assert!(validate_jail_name("my_jail").is_ok());
        assert!(validate_jail_name("jail123").is_ok());

        assert!(validate_jail_name("").is_err());
        assert!(validate_jail_name("a".repeat(256).as_str()).is_err());
        assert!(validate_jail_name("invalid name").is_err());
        assert!(validate_jail_name("invalid/name").is_err());
    }

    #[test]
    fn test_generate_jail_config() {
        let params = Params {
            name: "sshd".to_owned(),
            state: State::Present,
            enabled: true,
            port: Some("ssh".to_owned()),
            filter: Some("sshd".to_owned()),
            logpath: Some("/var/log/auth.log".to_owned()),
            maxretry: Some(5),
            findtime: Some(600),
            bantime: Some(3600),
            action: None,
        };

        let config = generate_jail_config(&params);
        assert!(config.contains("[sshd]"));
        assert!(config.contains("enabled = true"));
        assert!(config.contains("port = ssh"));
        assert!(config.contains("filter = sshd"));
        assert!(config.contains("logpath = /var/log/auth.log"));
        assert!(config.contains("maxretry = 5"));
        assert!(config.contains("findtime = 600"));
        assert!(config.contains("bantime = 3600"));
    }

    #[test]
    fn test_generate_jail_config_minimal() {
        let params = Params {
            name: "sshd".to_owned(),
            state: State::Present,
            enabled: false,
            port: None,
            filter: None,
            logpath: None,
            maxretry: None,
            findtime: None,
            bantime: None,
            action: None,
        };

        let config = generate_jail_config(&params);
        assert!(config.contains("[sshd]"));
        assert!(config.contains("enabled = false"));
        assert!(!config.contains("port"));
        assert!(!config.contains("filter"));
    }
}
