/// ANCHOR: module
/// # passwordstore
///
/// Manage passwords using pass (password-store), the standard Unix password manager.
///
/// Pass uses GPG for encryption and Git for version control. This module enables
/// secure credential management in scripts, container entrypoints, and IoT devices.
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
/// - name: Read a password from the store
///   passwordstore:
///     path: myapp/database
///     state: present
///   register: db_password
///
/// - name: Read all password data (password + metadata)
///   passwordstore:
///     path: myapp/database
///     returnall: true
///     state: present
///   register: db_full
///
/// - name: Create a new password entry
///   passwordstore:
///     path: myapp/api-key
///     password: "{{ api_key }}"
///     state: present
///
/// - name: Create a password with multiline content
///   passwordstore:
///     path: myapp/database
///     userpass: |
///       s3cret_p4ssw0rd
///       username: admin
///       url: db.example.com
///     state: present
///
/// - name: Generate a random password
///   passwordstore:
///     path: myapp/new-service
///     generate: true
///     length: 32
///     state: present
///
/// - name: Delete a password
///   passwordstore:
///     path: myapp/old-service
///     state: absent
///
/// - name: Use a custom password-store directory
///   passwordstore:
///     path: myapp/database
///     passwordstore: /opt/password-store
///     state: present
///   register: result
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the password in the password store.
    pub path: String,
    /// Whether the password should be present or absent.
    /// When present and password exists, returns the password content.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// The password to store. Required for state=present when creating a new entry
    /// (unless `generate` is true or `userpass` is provided).
    pub password: Option<String>,
    /// The full content of the password file (multiline). First line is the password,
    /// remaining lines are metadata. Mutually exclusive with `password`.
    pub userpass: Option<String>,
    /// Path to the password-store directory. Overrides PASSWORD_STORE_DIR environment variable.
    pub passwordstore: Option<String>,
    /// Generate a random password instead of providing one.
    /// The generated password will be stored in pass.
    #[serde(default)]
    pub generate: bool,
    /// Length of the generated password. Only used with `generate: true`.
    /// **[default: `16`]**
    #[serde(default = "default_length")]
    pub length: u32,
    /// Return all content from the password entry, not just the first line.
    #[serde(default)]
    pub returnall: bool,
}

fn default_length() -> u32 {
    16
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn run_pass_command(args: &[&str], store_dir: Option<&str>, input: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("pass");
    cmd.args(args);

    if let Some(dir) = store_dir {
        cmd.env("PASSWORD_STORE_DIR", dir);
    }

    let output = if let Some(data) = input {
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute pass command: {e}"),
                )
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(data.as_bytes()).map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to write to pass stdin: {e}"),
                )
            })?;
        }

        child.wait_with_output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to wait for pass command: {e}"),
            )
        })?
    } else {
        cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute pass command: {e}"),
            )
        })?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("pass command failed: {stderr}"),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn password_exists(path: &str, store_dir: Option<&str>) -> bool {
    run_pass_command(&["show", path], store_dir, None).is_ok()
}

fn read_password(path: &str, store_dir: Option<&str>) -> Result<String> {
    let output = run_pass_command(&["show", path], store_dir, None)?;
    Ok(output.trim_end().to_string())
}

fn insert_password(
    path: &str,
    content: &str,
    multiline: bool,
    store_dir: Option<&str>,
) -> Result<()> {
    let mut args = vec!["insert", "-f"];
    if multiline {
        args.push("-m");
    }
    args.push(path);

    run_pass_command(&args, store_dir, Some(content))?;
    Ok(())
}

fn generate_password(path: &str, length: u32, store_dir: Option<&str>) -> Result<String> {
    run_pass_command(
        &[
            "generate",
            "--force",
            "--no-symbols",
            path,
            &length.to_string(),
        ],
        store_dir,
        None,
    )?;

    let generated = read_password(path, store_dir)?;
    Ok(generated.lines().next().unwrap_or_default().to_string())
}

fn remove_password(path: &str, store_dir: Option<&str>) -> Result<()> {
    run_pass_command(&["rm", "--force", path], store_dir, None)?;
    Ok(())
}

pub fn passwordstore(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let store_dir = params.passwordstore.as_deref();

    match state {
        State::Present => exec_present(&params, store_dir, check_mode),
        State::Absent => exec_absent(&params, store_dir, check_mode),
    }
}

fn exec_present(
    params: &Params,
    store_dir: Option<&str>,
    check_mode: bool,
) -> Result<ModuleResult> {
    if params.password.is_some() && params.userpass.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "'password' and 'userpass' are mutually exclusive",
        ));
    }

    let exists = password_exists(&params.path, store_dir);

    if exists && !params.generate {
        let content = read_password(&params.path, store_dir)?;
        let password = if params.returnall {
            content.clone()
        } else {
            content.lines().next().unwrap_or_default().to_string()
        };

        let mut extra_data = json!({
            "path": params.path,
            "password": password,
        });

        if params.returnall {
            extra_data["content"] = json!(content);
        }

        return Ok(ModuleResult {
            changed: false,
            output: Some(password),
            extra: Some(value::to_value(extra_data)?),
        });
    }

    if params.password.is_none() && params.userpass.is_none() && !params.generate && !exists {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "One of 'password', 'userpass', or 'generate' is required when creating a new password entry",
        ));
    }

    if check_mode {
        let action = if params.generate {
            format!(
                "Would generate random password (length {}) for {}",
                params.length, params.path
            )
        } else {
            format!(
                "Would {} password for {}",
                if exists { "update" } else { "create" },
                params.path
            )
        };
        return Ok(ModuleResult {
            changed: true,
            output: Some(action),
            extra: None,
        });
    }

    if params.generate {
        let generated = generate_password(&params.path, params.length, store_dir)?;

        let extra_data = json!({
            "path": params.path,
            "password": generated,
            "generated": true,
        });

        return Ok(ModuleResult {
            changed: true,
            output: Some(generated),
            extra: Some(value::to_value(extra_data)?),
        });
    }

    if let Some(ref userpass) = params.userpass {
        insert_password(&params.path, userpass, true, store_dir)?;

        let password = userpass.lines().next().unwrap_or_default().to_string();
        let output = if params.returnall {
            userpass.clone()
        } else {
            password.clone()
        };

        let extra_data = json!({
            "path": params.path,
            "password": password,
        });

        return Ok(ModuleResult {
            changed: true,
            output: Some(output),
            extra: Some(value::to_value(extra_data)?),
        });
    }

    if let Some(ref password) = params.password {
        insert_password(&params.path, password, false, store_dir)?;

        let extra_data = json!({
            "path": params.path,
            "password": password,
        });

        return Ok(ModuleResult {
            changed: true,
            output: Some(password.clone()),
            extra: Some(value::to_value(extra_data)?),
        });
    }

    Err(Error::new(
        ErrorKind::InvalidData,
        format!(
            "No action specified for password {}: provide 'password', 'userpass', or 'generate'",
            params.path
        ),
    ))
}

fn exec_absent(params: &Params, store_dir: Option<&str>, check_mode: bool) -> Result<ModuleResult> {
    if !password_exists(&params.path, store_dir) {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Password {} does not exist", params.path)),
            extra: None,
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would remove password {}", params.path)),
            extra: None,
        });
    }

    remove_password(&params.path, store_dir)?;

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Password {} removed successfully", params.path)),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Passwordstore;

impl Module for Passwordstore {
    fn get_name(&self) -> &str {
        "passwordstore"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            passwordstore(parse_params(optional_params)?, check_mode)?,
            None,
        ))
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
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/database
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "myapp/database");
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/old-service
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_password() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/api-key
            password: s3cret
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.password, Some("s3cret".to_string()));
    }

    #[test]
    fn test_parse_params_with_userpass() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/database
            userpass: |
              s3cret_p4ssw0rd
              username: admin
              url: db.example.com
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.userpass.is_some());
        let userpass = params.userpass.unwrap();
        assert!(userpass.contains("s3cret_p4ssw0rd"));
        assert!(userpass.contains("username: admin"));
    }

    #[test]
    fn test_parse_params_with_store_dir() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/database
            passwordstore: /opt/password-store
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.passwordstore,
            Some("/opt/password-store".to_string())
        );
    }

    #[test]
    fn test_parse_params_generate() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/new-service
            generate: true
            length: 32
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.generate);
        assert_eq!(params.length, 32);
    }

    #[test]
    fn test_parse_params_returnall() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/database
            returnall: true
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.returnall);
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/database
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, None);
        assert!(!params.generate);
        assert_eq!(params.length, 16);
        assert!(!params.returnall);
        assert!(params.password.is_none());
        assert!(params.userpass.is_none());
        assert!(params.passwordstore.is_none());
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: myapp/database
            unknown_field: value
            state: present
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_state() {
        let state: State = Default::default();
        assert_eq!(state, State::Present);
    }

    #[test]
    fn test_default_length() {
        assert_eq!(default_length(), 16);
    }

    #[test]
    fn test_exec_present_mutually_exclusive() {
        let params = Params {
            path: "test".to_string(),
            state: Some(State::Present),
            password: Some("pass".to_string()),
            userpass: Some("content".to_string()),
            passwordstore: None,
            generate: false,
            length: 16,
            returnall: false,
        };
        let result = exec_present(&params, None, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_exec_present_missing_password() {
        let params = Params {
            path: "test".to_string(),
            state: Some(State::Present),
            password: None,
            userpass: None,
            passwordstore: None,
            generate: false,
            length: 16,
            returnall: false,
        };
        let result = exec_present(&params, None, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
