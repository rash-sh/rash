/// ANCHOR: module
/// # docker_login
///
/// Manage Docker registry authentication.
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
/// - name: Login to Docker Hub
///   docker_login:
///     username: myuser
///     password: mypassword
///
/// - name: Login to private registry
///   docker_login:
///     registry: registry.example.com
///     username: deploy
///     password: "{{ registry_password }}"
///
/// - name: Login with email
///   docker_login:
///     registry: registry.example.com
///     username: deploy
///     password: "{{ registry_password }}"
///     email: deploy@example.com
///
/// - name: Logout from Docker Hub
///   docker_login:
///     state: absent
///
/// - name: Logout from private registry
///   docker_login:
///     registry: registry.example.com
///     state: absent
///
/// - name: Re-authorize (force login even if already logged in)
///   docker_login:
///     registry: registry.example.com
///     username: deploy
///     password: "{{ registry_password }}"
///     reauthorize: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Registry URL (default: Docker Hub).
    #[serde(default)]
    registry: Option<String>,
    /// Username for authentication.
    #[serde(default)]
    username: Option<String>,
    /// Password for authentication.
    #[serde(default)]
    password: Option<String>,
    /// Email address for the registry account.
    #[serde(default)]
    email: Option<String>,
    /// Desired state of the registry login.
    #[serde(default)]
    state: State,
    /// Force re-authorization even if already logged in.
    #[serde(default)]
    reauthorize: bool,
}

#[derive(Debug)]
pub struct DockerLogin;

struct DockerClient {
    check_mode: bool,
}

impl Module for DockerLogin {
    fn get_name(&self) -> &str {
        "docker_login"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_login(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

impl DockerClient {
    fn new(check_mode: bool) -> Self {
        DockerClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("docker")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `docker {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing docker: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn is_logged_in(&self, registry: Option<&str>) -> Result<bool> {
        let registry_arg = registry.unwrap_or("https://index.docker.io/v1/");

        let output = self.exec_cmd(&["credential", "list"], false)?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains(registry_arg) || stdout.contains("index.docker.io") {
                return Ok(true);
            }
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains(registry_arg) || stdout.contains("index.docker.io"))
    }

    fn check_config_auth(&self, registry: Option<&str>) -> Result<bool> {
        let output = self.exec_cmd(
            &["config", "list", "--format", "{{.CredentialsStore}}"],
            false,
        )?;

        if !output.status.success() {
            return self.is_logged_in(registry);
        }

        self.is_logged_in(registry)
    }

    fn login(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["login".to_string()];

        if let Some(ref registry) = params.registry {
            args.push(registry.clone());
        }

        if let Some(ref username) = params.username {
            args.push("-u".to_string());
            args.push(username.clone());
        }

        if let Some(ref password) = params.password {
            args.push("-p".to_string());
            args.push(password.clone());
        }

        if let Some(ref email) = params.email {
            args.push("-e".to_string());
            args.push(email.clone());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;

        Ok(output.status.success())
    }

    fn logout(&self, registry: Option<&str>) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<&str> = vec!["logout"];
        if let Some(reg) = registry {
            args.push(reg);
        }

        let output = self.exec_cmd(&args, false)?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() && !stderr.contains("Not logged in") {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error logging out: {}", stderr),
            ));
        }

        Ok(true)
    }
}

fn docker_login(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    let registry_name = params.registry.as_deref().unwrap_or("Docker Hub");

    match params.state {
        State::Present => {
            let username = params.username.as_ref().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "username is required for login")
            })?;

            let _password = params.password.as_ref().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "password is required for login")
            })?;

            let is_logged_in = client.check_config_auth(params.registry.as_deref())?;

            if !is_logged_in || params.reauthorize {
                if params.reauthorize && is_logged_in {
                    diff(
                        format!("registry: {} (logged in)", registry_name),
                        format!("registry: {} (re-authorizing)", registry_name),
                    );
                } else {
                    diff(
                        format!("registry: {} (not logged in)", registry_name),
                        format!("registry: {} (logged in)", registry_name),
                    );
                }

                client.login(&params)?;
                output_messages.push(format!(
                    "Successfully logged in to {} as {}",
                    registry_name, username
                ));
                changed = true;
            } else {
                output_messages.push(format!("Already logged in to {}", registry_name));
            }
        }
        State::Absent => {
            let is_logged_in = client.check_config_auth(params.registry.as_deref())?;

            if is_logged_in {
                diff(
                    format!("registry: {} (logged in)", registry_name),
                    format!("registry: {} (logged out)", registry_name),
                );
                client.logout(params.registry.as_deref())?;
                output_messages.push(format!("Successfully logged out from {}", registry_name));
                changed = true;
            } else {
                output_messages.push(format!("Not logged in to {}", registry_name));
            }
        }
    }

    let extra: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult {
        changed,
        output: final_output,
        extra: if extra.is_empty() {
            None
        } else {
            Some(serde_norway::value::to_value(extra)?)
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            username: myuser
            password: mypassword
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.username, Some("myuser".to_string()));
        assert_eq!(params.password, Some("mypassword".to_string()));
        assert_eq!(params.state, State::Present);
        assert_eq!(params.registry, None);
    }

    #[test]
    fn test_parse_params_with_registry() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            registry: registry.example.com
            username: deploy
            password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.registry, Some("registry.example.com".to_string()));
        assert_eq!(params.username, Some("deploy".to_string()));
        assert_eq!(params.password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_params_logout() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_logout_with_registry() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            registry: registry.example.com
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.registry, Some("registry.example.com".to_string()));
    }

    #[test]
    fn test_parse_params_with_email() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            registry: registry.example.com
            username: deploy
            password: secret
            email: deploy@example.com
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.email, Some("deploy@example.com".to_string()));
    }

    #[test]
    fn test_parse_params_reauthorize() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            registry: registry.example.com
            username: deploy
            password: secret
            reauthorize: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.reauthorize);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            username: myuser
            password: mypassword
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_login_requires_username() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            password: mypassword
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = docker_login(params, true);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_login_requires_password() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            username: myuser
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = docker_login(params, true);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
