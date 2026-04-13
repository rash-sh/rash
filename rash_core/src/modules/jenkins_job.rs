/// ANCHOR: module
/// # jenkins_job
///
/// Manage Jenkins jobs and builds.
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
/// - name: Create Jenkins job
///   jenkins_job:
///     name: myapp-build
///     state: present
///     url: http://jenkins.local
///     user: admin
///     password: secret
///
/// - name: Create Jenkins job with config XML
///   jenkins_job:
///     name: myapp-build
///     state: present
///     url: http://jenkins.local
///     user: admin
///     password: secret
///     config: |
///       <project>
///         <description>My app build job</description>
///         <builders>
///           <hudson.tasks.Shell>
///             <command>echo "Building"</command>
///           </hudson.tasks.Shell>
///         </builders>
///       </project>
///
/// - name: Trigger Jenkins build
///   jenkins_job:
///     name: myapp-build
///     state: present
///     url: http://jenkins.local
///     user: admin
///     password: secret
///     enabled: true
///
/// - name: Delete Jenkins job
///   jenkins_job:
///     name: old-job
///     state: absent
///     url: http://jenkins.local
///     user: admin
///     password: secret
///
/// - name: Trigger build with token
///   jenkins_job:
///     name: myapp-build
///     state: present
///     url: http://jenkins.local
///     user: admin
///     password: secret
///     token: build-token
///     enabled: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::time::Duration;

use minijinja::Value;
use reqwest::blocking::Client;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the Jenkins job.
    pub name: String,
    /// Jenkins server URL.
    pub url: String,
    /// Jenkins username for authentication.
    pub user: String,
    /// Jenkins password or API token.
    pub password: String,
    /// Whether the job should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Job configuration XML content.
    pub config: Option<String>,
    /// Build token for triggering builds.
    pub token: Option<String>,
    /// Whether to trigger a build (only for state=present).
    #[serde(default)]
    pub enabled: bool,
    /// Timeout in seconds for API requests.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// If false, SSL certificates will not be validated.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn default_timeout() -> u64 {
    30
}

fn default_validate_certs() -> bool {
    true
}

fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    }
}

fn create_client(params: &Params) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(params.timeout))
        .danger_accept_invalid_certs(!params.validate_certs)
        .build()
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create HTTP client: {e}"),
            )
        })
}

fn check_job_exists(
    client: &Client,
    url: &str,
    name: &str,
    user: &str,
    password: &str,
) -> Result<bool> {
    let job_url = format!("{url}job/{name}/api/json");
    let response = client
        .get(&job_url)
        .basic_auth(user, Some(password))
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to check job existence: {e}"),
            )
        })?;

    Ok(response.status().as_u16() == 200)
}

fn create_job(
    client: &Client,
    url: &str,
    name: &str,
    config: &str,
    user: &str,
    password: &str,
) -> Result<()> {
    let create_url = format!("{url}createItem?name={name}");
    let response = client
        .post(&create_url)
        .basic_auth(user, Some(password))
        .header("Content-Type", "application/xml")
        .body(config.to_string())
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create Jenkins job: {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read response body: {e}"),
            )
        })?;
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to create job '{name}': HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    Ok(())
}

fn delete_job(client: &Client, url: &str, name: &str, user: &str, password: &str) -> Result<()> {
    let delete_url = format!("{url}job/{name}/doDelete");
    let response = client
        .post(&delete_url)
        .basic_auth(user, Some(password))
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to delete Jenkins job: {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() && status.as_u16() != 302 {
        let body = response.text().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read response body: {e}"),
            )
        })?;
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to delete job '{name}': HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    Ok(())
}

fn trigger_build(
    client: &Client,
    url: &str,
    name: &str,
    token: Option<&str>,
    user: &str,
    password: &str,
) -> Result<()> {
    let build_url = if let Some(tok) = token {
        format!("{url}job/{name}/build?token={tok}")
    } else {
        format!("{url}job/{name}/build")
    };

    let response = client
        .post(&build_url)
        .basic_auth(user, Some(password))
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to trigger Jenkins build: {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() && status.as_u16() != 201 && status.as_u16() != 302 {
        let body = response.text().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read response body: {e}"),
            )
        })?;
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to trigger build for '{name}': HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    Ok(())
}

fn get_default_config() -> String {
    r#"<?xml version='1.1' encoding='UTF-8'?>
<project>
  <description></description>
  <keepDependencies>false</keepDependencies>
  <properties/>
  <scm class="hudson.scm.NullSCM"/>
  <canRoam>true</canRoam>
  <disabled>false</disabled>
  <blockBuildWhenDownstreamBuilding>false</blockBuildWhenDownstreamBuilding>
  <blockBuildWhenUpstreamBuilding>false</blockBuildWhenUpstreamBuilding>
  <triggers/>
  <concurrentBuild>false</concurrentBuild>
  <builders/>
  <publishers/>
  <buildWrappers/>
</project>"#
        .to_string()
}

pub fn jenkins_job(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let url = normalize_url(&params.url);

    let client = create_client(&params)?;

    match state {
        State::Present => {
            let job_exists =
                check_job_exists(&client, &url, &params.name, &params.user, &params.password)?;

            let config = params.config.clone().unwrap_or_else(get_default_config);

            if !job_exists {
                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Would create Jenkins job '{}'", params.name)),
                        extra: None,
                    });
                }

                create_job(
                    &client,
                    &url,
                    &params.name,
                    &config,
                    &params.user,
                    &params.password,
                )?;

                let extra = json!({
                    "name": params.name,
                    "url": params.url,
                    "state": "present",
                    "created": true,
                });

                let output = format!("Created Jenkins job '{}'", params.name);

                if params.enabled {
                    trigger_build(
                        &client,
                        &url,
                        &params.name,
                        params.token.as_deref(),
                        &params.user,
                        &params.password,
                    )?;
                }

                return Ok(ModuleResult {
                    changed: true,
                    output: Some(output),
                    extra: Some(value::to_value(extra)?),
                });
            }

            if params.enabled {
                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!(
                            "Would trigger build for Jenkins job '{}'",
                            params.name
                        )),
                        extra: None,
                    });
                }

                trigger_build(
                    &client,
                    &url,
                    &params.name,
                    params.token.as_deref(),
                    &params.user,
                    &params.password,
                )?;

                let extra = json!({
                    "name": params.name,
                    "url": params.url,
                    "state": "present",
                    "build_triggered": true,
                });

                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Triggered build for Jenkins job '{}'", params.name)),
                    extra: Some(value::to_value(extra)?),
                });
            }

            let extra = json!({
                "name": params.name,
                "url": params.url,
                "state": "present",
                "exists": true,
            });

            Ok(ModuleResult {
                changed: false,
                output: Some(format!("Jenkins job '{}' already exists", params.name)),
                extra: Some(value::to_value(extra)?),
            })
        }
        State::Absent => {
            let job_exists =
                check_job_exists(&client, &url, &params.name, &params.user, &params.password)?;

            if !job_exists {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Jenkins job '{}' does not exist", params.name)),
                    extra: None,
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would delete Jenkins job '{}'", params.name)),
                    extra: None,
                });
            }

            delete_job(&client, &url, &params.name, &params.user, &params.password)?;

            let extra = json!({
                "name": params.name,
                "url": params.url,
                "state": "absent",
                "deleted": true,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Deleted Jenkins job '{}'", params.name)),
                extra: Some(value::to_value(extra)?),
            })
        }
    }
}

#[derive(Debug)]
pub struct JenkinsJob;

impl Module for JenkinsJob {
    fn get_name(&self) -> &str {
        "jenkins_job"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((jenkins_job(parse_params(params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_norway::from_str;

    #[test]
    fn test_parse_params_basic() {
        let yaml = r#"
name: myapp-build
url: http://jenkins.local
user: admin
password: secret
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "myapp-build");
        assert_eq!(params.url, "http://jenkins.local");
        assert_eq!(params.user, "admin");
        assert_eq!(params.password, "secret");
        assert_eq!(params.state, None);
        assert!(!params.enabled);
        assert_eq!(params.timeout, 30);
        assert!(params.validate_certs);
    }

    #[test]
    fn test_parse_params_with_config() {
        let yaml = r#"
name: myapp-build
url: http://jenkins.local
user: admin
password: secret
state: present
config: |
  <project>
    <description>My job</description>
  </project>
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.state, Some(State::Present));
        assert!(params.config.is_some());
        assert!(params.config.unwrap().contains("<project>"));
    }

    #[test]
    fn test_parse_params_with_state_absent() {
        let yaml = r#"
name: old-job
url: http://jenkins.local
user: admin
password: secret
state: absent
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_build_trigger() {
        let yaml = r#"
name: myapp-build
url: http://jenkins.local
user: admin
password: secret
enabled: true
token: build-token
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert!(params.enabled);
        assert_eq!(params.token, Some("build-token".to_string()));
    }

    #[test]
    fn test_parse_params_with_timeout() {
        let yaml = r#"
name: myapp-build
url: http://jenkins.local
user: admin
password: secret
timeout: 60
validate_certs: false
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.timeout, 60);
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("http://jenkins.local"),
            "http://jenkins.local/"
        );
        assert_eq!(
            normalize_url("http://jenkins.local/"),
            "http://jenkins.local/"
        );
        assert_eq!(
            normalize_url("http://jenkins.local "),
            "http://jenkins.local/"
        );
    }

    #[test]
    fn test_default_state() {
        let state: State = Default::default();
        assert_eq!(state, State::Present);
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml = r#"
name: myapp-build
url: http://jenkins.local
user: admin
password: secret
unknown_field: value
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let error = parse_params::<Params>(value).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_get_default_config() {
        let config = get_default_config();
        assert!(config.contains("<project>"));
        assert!(config.contains("<?xml"));
    }
}
