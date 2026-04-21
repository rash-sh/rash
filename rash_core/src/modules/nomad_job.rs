/// ANCHOR: module
/// # nomad_job
///
/// Deploy and manage HashiCorp Nomad jobs.
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
/// - name: Deploy a Nomad job
///   nomad_job:
///     name: webapp
///     spec: /etc/nomad/jobs/webapp.nomad
///     state: present
///
/// - name: Deploy job with custom Nomad URL
///   nomad_job:
///     name: webapp
///     spec: /etc/nomad/jobs/webapp.nomad
///     url: http://nomad.example.com:4646
///     state: present
///
/// - name: Deploy job with ACL token
///   nomad_job:
///     name: webapp
///     spec: /etc/nomad/jobs/webapp.nomad
///     token: "{{ nomad_token }}"
///     state: present
///
/// - name: Remove a Nomad job
///   nomad_job:
///     name: webapp
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_norway::Value as YamlValue;

use std::fs;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The name of the Nomad job.
    pub name: String,
    /// Path to the Nomad job specification file (JSON format).
    /// Required when state=present.
    pub spec: Option<String>,
    /// The desired state of the job.
    #[serde(default)]
    pub state: State,
    /// The Nomad API URL.
    #[serde(default = "default_url")]
    pub url: String,
    /// ACL token for authentication.
    pub token: Option<String>,
}

fn default_url() -> String {
    "http://localhost:4646".to_string()
}

struct NomadClient {
    url: String,
    token: Option<String>,
}

impl NomadClient {
    fn new(params: &Params) -> Self {
        Self {
            url: params.url.clone(),
            token: params.token.clone(),
        }
    }

    fn build_client(&self) -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder().build().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create HTTP client: {e}"),
            )
        })
    }

    fn add_token_header(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some(ref token) = self.token {
            request.header("X-Nomad-Token", token)
        } else {
            request
        }
    }

    fn get_job(&self, name: &str) -> Result<Option<JsonValue>> {
        let url = format!("{}/v1/job/{}", self.url, name);
        let client = self.build_client()?;
        let request = self.add_token_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad API request failed: {e}"),
            )
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad returned status {}: {}", status, error_text),
            ));
        }

        let job: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to parse Nomad response: {e}"),
            )
        })?;

        Ok(Some(job))
    }

    fn get_job_version(&self, name: &str) -> Result<Option<u64>> {
        let url = format!("{}/v1/job/{}?version=0", self.url, name);
        let client = self.build_client()?;
        let request = self.add_token_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad API request failed: {e}"),
            )
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad returned status {}: {}", status, error_text),
            ));
        }

        let job: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to parse Nomad response: {e}"),
            )
        })?;

        let version = job.get("Version").and_then(|v| v.as_u64());
        Ok(version)
    }

    fn deploy_job(&self, job_spec: &str) -> Result<JsonValue> {
        let url = format!("{}/v1/jobs", self.url);
        let client = self.build_client()?;

        let job_json: JsonValue = serde_json::from_str(job_spec).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse job spec as JSON: {e}"),
            )
        })?;

        let request = self.add_token_header(client.post(&url).json(&job_json));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad deploy request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad returned status {}: {}", status, error_text),
            ));
        }

        let result: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to parse Nomad response: {e}"),
            )
        })?;

        Ok(result)
    }

    fn delete_job(&self, name: &str) -> Result<bool> {
        let url = format!("{}/v1/job/{}", self.url, name);
        let client = self.build_client()?;
        let request = self.add_token_header(client.delete(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad delete request failed: {e}"),
            )
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Nomad returned status {}: {}", status, error_text),
            ));
        }

        Ok(true)
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let spec_path = params.spec.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "spec parameter is required when state=present",
        )
    })?;

    let job_spec = fs::read_to_string(spec_path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read job spec file '{}': {e}", spec_path),
        )
    })?;

    let client = NomadClient::new(params);

    let existing_version = client.get_job_version(&params.name)?;

    if let Some(current_version) = existing_version {
        let pre_deploy_version = current_version;

        if check_mode {
            return Ok(ModuleResult::new(
                true,
                Some(serde_norway::value::to_value(json!({
                    "name": params.name,
                    "spec": params.spec,
                    "current_version": current_version,
                    "changed": true
                }))?),
                Some(format!("Job {} would be updated (check mode)", params.name)),
            ));
        }

        let result = client.deploy_job(&job_spec)?;

        let new_version = result
            .get("JobModifyIndex")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let changed = new_version != pre_deploy_version;

        Ok(ModuleResult::new(
            changed,
            Some(serde_norway::value::to_value(json!({
                "name": params.name,
                "spec": params.spec,
                "previous_version": pre_deploy_version,
                "current_version": new_version,
                "eval_id": result.get("EvalID").and_then(|v| v.as_str()).unwrap_or(""),
            }))?),
            Some(format!(
                "Job {} {} (version {})",
                params.name,
                if changed { "updated" } else { "unchanged" },
                new_version
            )),
        ))
    } else {
        if check_mode {
            return Ok(ModuleResult::new(
                true,
                Some(serde_norway::value::to_value(json!({
                    "name": params.name,
                    "spec": params.spec,
                    "changed": true
                }))?),
                Some(format!("Job {} would be created (check mode)", params.name)),
            ));
        }

        let result = client.deploy_job(&job_spec)?;

        let new_version = result
            .get("JobModifyIndex")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(ModuleResult::new(
            true,
            Some(serde_norway::value::to_value(json!({
                "name": params.name,
                "spec": params.spec,
                "current_version": new_version,
                "eval_id": result.get("EvalID").and_then(|v| v.as_str()).unwrap_or(""),
            }))?),
            Some(format!(
                "Job {} created (version {})",
                params.name, new_version
            )),
        ))
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = NomadClient::new(params);

    let existing = client.get_job(&params.name)?;

    match existing {
        Some(_) => {
            if check_mode {
                return Ok(ModuleResult::new(
                    true,
                    Some(serde_norway::value::to_value(json!({
                        "name": params.name,
                        "changed": true
                    }))?),
                    Some(format!("Job {} would be deleted (check mode)", params.name)),
                ));
            }

            let deleted = client.delete_job(&params.name)?;

            Ok(ModuleResult::new(
                deleted,
                Some(serde_norway::value::to_value(json!({
                    "name": params.name,
                    "deleted": deleted
                }))?),
                Some(if deleted {
                    format!("Job {} deleted", params.name)
                } else {
                    format!("Job {} not found", params.name)
                }),
            ))
        }
        None => Ok(ModuleResult::new(
            false,
            Some(serde_norway::value::to_value(json!({
                "name": params.name,
                "deleted": false
            }))?),
            Some(format!("Job {} does not exist", params.name)),
        )),
    }
}

pub fn nomad_job(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct NomadJob;

impl Module for NomadJob {
    fn get_name(&self) -> &str {
        "nomad_job"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((nomad_job(parse_params(optional_params)?, check_mode)?, None))
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
            name: webapp
            spec: /etc/nomad/jobs/webapp.nomad
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webapp");
        assert_eq!(
            params.spec,
            Some("/etc/nomad/jobs/webapp.nomad".to_string())
        );
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webapp");
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.spec, None);
    }

    #[test]
    fn test_parse_params_with_url() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            spec: /etc/nomad/jobs/webapp.nomad
            url: http://nomad.example.com:4646
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.url, "http://nomad.example.com:4646");
    }

    #[test]
    fn test_parse_params_with_token() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            spec: /etc/nomad/jobs/webapp.nomad
            token: my-secret-token
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.token, Some("my-secret-token".to_string()));
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.url, "http://localhost:4646");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.token, None);
        assert_eq!(params.spec, None);
    }

    #[test]
    fn test_parse_params_rejects_unknown_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            spec: /etc/nomad/jobs/webapp.nomad
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_nomad_client_build_url() {
        let params = Params {
            name: "test".to_string(),
            spec: None,
            state: State::Present,
            url: "http://nomad.example.com:4646".to_string(),
            token: None,
        };
        let client = NomadClient::new(&params);
        assert_eq!(client.url, "http://nomad.example.com:4646");
    }
}
