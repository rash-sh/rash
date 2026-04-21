/// ANCHOR: module
/// # mongodb_replicaset
///
/// Manage MongoDB replica sets.
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
/// - name: Initialize a replica set
///   mongodb_replicaset:
///     repl_set: rs0
///     state: initialized
///     members:
///       - mongo1:27017
///       - mongo2:27017
///       - mongo3:27017
///     login_user: admin
///     login_password: secret
///
/// - name: Initialize replica set on localhost
///   mongodb_replicaset:
///     repl_set: rs0
///     state: initialized
///     members:
///       - localhost:27017
///
/// - name: Add member to replica set
///   mongodb_replicaset:
///     repl_set: rs0
///     state: present
///     members:
///       - mongo1:27017
///       - mongo2:27017
///       - mongo3:27017
///       - mongo4:27017
///     login_user: admin
///     login_password: secret
///
/// - name: Remove member from replica set
///   mongodb_replicaset:
///     repl_set: rs0
///     state: present
///     members:
///       - mongo1:27017
///       - mongo2:27017
///     login_user: admin
///     login_password: secret
///
/// - name: Check replica set status
///   mongodb_replicaset:
///     repl_set: rs0
///     state: present
///     login_user: admin
///     login_password: secret
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
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::process::Command;

fn default_state() -> State {
    State::Present
}

fn default_login_host() -> String {
    "localhost".to_string()
}

fn default_login_port() -> u16 {
    27017
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
    Initialized,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Present => write!(f, "present"),
            State::Absent => write!(f, "absent"),
            State::Initialized => write!(f, "initialized"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Replica set name.
    pub repl_set: String,
    /// The desired state of the replica set.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// List of replica set members (host:port format).
    pub members: Option<Vec<String>>,
    /// Database host to connect to.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_login_host")]
    pub login_host: String,
    /// Database port to connect to.
    /// **[default: `27017`]**
    #[serde(default = "default_login_port")]
    pub login_port: u16,
    /// Database user to connect with.
    pub login_user: Option<String>,
    /// Database password to use.
    pub login_password: Option<String>,
    /// Authentication database.
    #[serde(default = "default_auth_database")]
    pub auth_database: String,
}

fn default_auth_database() -> String {
    "admin".to_string()
}

fn build_mongo_uri(params: &Params) -> String {
    let mut uri = "mongodb://".to_string();

    if let Some(ref user) = params.login_user {
        uri.push_str(user);
        if let Some(ref password) = params.login_password {
            uri.push(':');
            uri.push_str(password);
        }
        uri.push('@');
    }

    uri.push_str(&params.login_host);
    uri.push(':');
    uri.push_str(&params.login_port.to_string());

    uri.push('/');
    uri.push_str(&params.auth_database);

    uri
}

fn run_mongo_command(params: &Params, command: &str) -> Result<String> {
    let uri = build_mongo_uri(params);

    let eval = format!("JSON.stringify({})", command);

    trace!("Running mongosh command: {}", command);

    let output = Command::new("mongosh")
        .arg("--quiet")
        .arg("--eval")
        .arg(&eval)
        .arg(&uri)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::new(
                    ErrorKind::NotFound,
                    "mongosh command not found. Please install MongoDB Shell.",
                )
            } else {
                Error::new(ErrorKind::SubprocessFail, e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("MongoDB command failed: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().to_string())
}

fn is_replica_set_initialized(params: &Params) -> Result<bool> {
    let result = run_mongo_command(
        params,
        "db.getMongo().adminCommand('replSetGetStatus').ok === 1",
    )?;

    Ok(result == "true")
}

fn get_current_members(params: &Params) -> Result<Vec<String>> {
    let result = run_mongo_command(params, "rs.conf().members.map(m => m.host)")?;

    if result.is_empty() || result == "null" {
        return Ok(Vec::new());
    }

    let members: Vec<String> = serde_json::from_str(&result).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse replica set members: {}", e),
        )
    })?;

    Ok(members)
}

fn initialize_replicaset(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if is_replica_set_initialized(params)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!(
                "Replica set '{}' is already initialized",
                params.repl_set
            )),
        ));
    }

    let members = params.members.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::OmitParam,
            "members is required for initializing a replica set",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!(
                "Would initialize replica set '{}' with members: {:?}",
                params.repl_set, members
            )),
        ));
    }

    let members_config: Vec<serde_json::Value> = members
        .iter()
        .enumerate()
        .map(|(i, host)| {
            serde_json::json!({
                "_id": i,
                "host": host
            })
        })
        .collect();

    let config = serde_json::json!({
        "_id": params.repl_set,
        "members": members_config
    });

    let config_str = serde_json::to_string(&config).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to serialize replica set config: {}", e),
        )
    })?;

    let command = format!("rs.initiate({})", config_str);

    run_mongo_command(params, &command)?;

    let extra = Some(value::to_value(json!({
        "repl_set": params.repl_set,
        "state": params.state.to_string(),
        "members": members,
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!(
            "Replica set '{}' initialized with {} member(s)",
            params.repl_set,
            members.len()
        )),
    ))
}

fn check_replicaset_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if !is_replica_set_initialized(params)? {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Replica set '{}' is not initialized. Use state 'initialized' first.",
                params.repl_set
            ),
        ));
    }

    let current_members = get_current_members(params)?;

    if let Some(ref desired_members) = params.members {
        let mut to_add: Vec<&String> = Vec::new();
        let mut to_remove: Vec<&String> = Vec::new();

        for member in desired_members {
            if !current_members.iter().any(|m| m == member) {
                to_add.push(member);
            }
        }

        for member in &current_members {
            if !desired_members.iter().any(|m| m == member) {
                to_remove.push(member);
            }
        }

        if to_add.is_empty() && to_remove.is_empty() {
            return Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "repl_set": params.repl_set,
                    "members": current_members,
                }))?),
                Some(format!(
                    "Replica set '{}' members are already in the desired state",
                    params.repl_set
                )),
            ));
        }

        if check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "Would update replica set '{}': add {:?}, remove {:?}",
                    params.repl_set, to_add, to_remove
                )),
            ));
        }

        for member in &to_add {
            let command = format!("rs.add('{}')", member);
            run_mongo_command(params, &command)?;
        }

        for member in &to_remove {
            let command = format!("rs.remove('{}')", member);
            run_mongo_command(params, &command)?;
        }

        let extra = Some(value::to_value(json!({
            "repl_set": params.repl_set,
            "state": params.state.to_string(),
            "added": to_add,
            "removed": to_remove,
        }))?);

        return Ok(ModuleResult::new(
            true,
            extra,
            Some(format!(
                "Replica set '{}' updated: added {} member(s), removed {} member(s)",
                params.repl_set,
                to_add.len(),
                to_remove.len()
            )),
        ));
    }

    let extra = Some(value::to_value(json!({
        "repl_set": params.repl_set,
        "members": current_members,
    }))?);

    Ok(ModuleResult::new(
        false,
        extra,
        Some(format!(
            "Replica set '{}' is present with {} member(s)",
            params.repl_set,
            current_members.len()
        )),
    ))
}

fn remove_replicaset(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if !is_replica_set_initialized(params)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!(
                "Replica set '{}' is not initialized",
                params.repl_set
            )),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would remove replica set '{}'", params.repl_set)),
        ));
    }

    run_mongo_command(params, "rs.stepDown()")?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!(
            "Replica set '{}' stepped down (replica sets cannot be fully removed without restarting)",
            params.repl_set
        )),
    ))
}

fn mongodb_replicaset_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Initialized => initialize_replicaset(&params, check_mode),
        State::Present => check_replicaset_present(&params, check_mode),
        State::Absent => remove_replicaset(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct MongodbReplicaset;

impl Module for MongodbReplicaset {
    fn get_name(&self) -> &str {
        "mongodb_replicaset"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mongodb_replicaset_impl(params, check_mode)?, None))
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
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repl_set: rs0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repl_set, "rs0");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 27017);
        assert_eq!(params.auth_database, "admin");
        assert!(params.members.is_none());
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repl_set: rs0
            state: initialized
            members:
              - mongo1:27017
              - mongo2:27017
              - mongo3:27017
            login_host: mongodb.example.com
            login_user: admin
            login_password: secret
            login_port: 27018
            auth_database: admin
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repl_set, "rs0");
        assert_eq!(params.state, State::Initialized);
        assert_eq!(
            params.members,
            Some(vec![
                "mongo1:27017".to_string(),
                "mongo2:27017".to_string(),
                "mongo3:27017".to_string(),
            ])
        );
        assert_eq!(params.login_host, "mongodb.example.com");
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("secret".to_string()));
        assert_eq!(params.login_port, 27018);
    }

    #[test]
    fn test_parse_params_present_with_members() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repl_set: rs0
            state: present
            members:
              - mongo1:27017
              - mongo2:27017
            login_user: admin
            login_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repl_set, "rs0");
        assert_eq!(params.state, State::Present);
        assert_eq!(
            params.members,
            Some(vec!["mongo1:27017".to_string(), "mongo2:27017".to_string(),])
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repl_set: rs0
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repl_set, "rs0");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_state_display() {
        assert_eq!(State::Present.to_string(), "present");
        assert_eq!(State::Absent.to_string(), "absent");
        assert_eq!(State::Initialized.to_string(), "initialized");
    }

    #[test]
    fn test_build_mongo_uri_basic() {
        let params = Params {
            repl_set: "rs0".to_string(),
            state: State::Present,
            members: None,
            login_host: "localhost".to_string(),
            login_port: 27017,
            login_user: None,
            login_password: None,
            auth_database: "admin".to_string(),
        };
        let uri = build_mongo_uri(&params);
        assert_eq!(uri, "mongodb://localhost:27017/admin");
    }

    #[test]
    fn test_build_mongo_uri_with_auth() {
        let params = Params {
            repl_set: "rs0".to_string(),
            state: State::Initialized,
            members: Some(vec!["mongo1:27017".to_string()]),
            login_host: "mongodb.example.com".to_string(),
            login_port: 27018,
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            auth_database: "admin".to_string(),
        };
        let uri = build_mongo_uri(&params);
        assert_eq!(
            uri,
            "mongodb://admin:secret@mongodb.example.com:27018/admin"
        );
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repl_set: rs0
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_initialized_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repl_set: rs0
            state: initialized
            members:
              - localhost:27017
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repl_set, "rs0");
        assert_eq!(params.state, State::Initialized);
        assert_eq!(params.members, Some(vec!["localhost:27017".to_string()]));
    }

    #[test]
    fn test_parse_params_single_member() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repl_set: rs0
            state: initialized
            members:
              - localhost:27017
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.members.unwrap().len(), 1);
    }
}
