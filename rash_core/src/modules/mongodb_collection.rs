/// ANCHOR: module
/// # mongodb_collection
///
/// Manage MongoDB collections.
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
/// - name: Create a collection
///   mongodb_collection:
///     name: users
///     database: myapp
///     state: present
///
/// - name: Create collection with indexes
///   mongodb_collection:
///     name: users
///     database: myapp
///     state: present
///     indexes:
///       - key: { email: 1 }
///         unique: true
///       - key: { created_at: -1 }
///         name: idx_created_at
///
/// - name: Create collection with validator
///   mongodb_collection:
///     name: users
///     database: myapp
///     state: present
///     validator:
///       $jsonSchema:
///         required: ["email"]
///         properties:
///           email:
///             bsonType: "string"
///     validation_level: strict
///     validation_action: error
///
/// - name: Create collection with collation
///   mongodb_collection:
///     name: users
///     database: myapp
///     state: present
///     collation:
///       locale: en
///       strength: 2
///
/// - name: Drop a collection
///   mongodb_collection:
///     name: logs
///     database: myapp
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
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::collections::HashMap;
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

fn default_auth_database() -> String {
    "admin".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Present => write!(f, "present"),
            State::Absent => write!(f, "absent"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ValidationLevel {
    Off,
    Strict,
    Moderate,
}

impl std::fmt::Display for ValidationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationLevel::Off => write!(f, "off"),
            ValidationLevel::Strict => write!(f, "strict"),
            ValidationLevel::Moderate => write!(f, "moderate"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ValidationAction {
    Error,
    Warn,
}

impl std::fmt::Display for ValidationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationAction::Error => write!(f, "error"),
            ValidationAction::Warn => write!(f, "warn"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct IndexParams {
    /// Key field(s) for the index as a map of field to direction.
    pub key: HashMap<String, serde_json::Value>,
    /// Make index unique.
    #[serde(default)]
    pub unique: bool,
    /// Custom index name.
    pub name: Option<String>,
    /// Create sparse index.
    #[serde(default)]
    pub sparse: bool,
    /// Background index creation.
    #[serde(default)]
    pub background: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the collection to manage.
    pub name: String,
    /// Name of the database containing the collection.
    pub database: String,
    /// The collection state.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// List of indexes to create on the collection.
    pub indexes: Option<Vec<IndexParams>>,
    /// Collection validator document.
    pub validator: Option<HashMap<String, serde_json::Value>>,
    /// Validation level (off/strict/moderate).
    pub validation_level: Option<ValidationLevel>,
    /// Validation action (error/warn).
    pub validation_action: Option<ValidationAction>,
    /// Collation settings for the collection.
    pub collation: Option<HashMap<String, serde_json::Value>>,
    /// Replica set name (for replica set connections).
    pub replica_set: Option<String>,
    /// Database host to connect to.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_login_host")]
    pub login_host: String,
    /// Database user to connect with.
    pub login_user: Option<String>,
    /// Database password to use.
    pub login_password: Option<String>,
    /// Database port to connect to.
    /// **[default: `27017`]**
    #[serde(default = "default_login_port")]
    pub login_port: u16,
    /// Connection options string.
    pub connection_options: Option<String>,
    /// Authentication database.
    /// **[default: `"admin"`]**
    #[serde(default = "default_auth_database")]
    pub auth_database: String,
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

    if let Some(ref replica_set) = params.replica_set {
        uri.push_str("?replicaSet=");
        uri.push_str(replica_set);
    }

    if let Some(ref options) = params.connection_options {
        if params.replica_set.is_some() {
            uri.push('&');
        } else {
            uri.push('?');
        }
        uri.push_str(options);
    }

    uri
}

fn run_mongo_command(params: &Params, command: &str, database: &str) -> Result<String> {
    let uri = build_mongo_uri(params);

    let eval = format!("JSON.stringify({})", command);

    trace!(
        "Running mongosh command: {} on database {}",
        command, database
    );

    let output = Command::new("mongosh")
        .arg("--quiet")
        .arg("--eval")
        .arg(&eval)
        .arg(&uri)
        .arg(database)
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

fn collection_exists(params: &Params) -> Result<bool> {
    let result = run_mongo_command(
        params,
        &format!("db.getCollectionNames().indexOf('{}') >= 0", params.name),
        &params.database,
    )?;

    Ok(result == "true")
}

fn hashmap_to_json_value(map: &HashMap<String, serde_json::Value>) -> serde_json::Value {
    serde_json::Value::Object(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn create_collection(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if collection_exists(params)? {
        let mut changed = false;
        let mut changes: Vec<String> = Vec::new();

        if let Some(ref indexes) = params.indexes {
            let result = create_indexes_internal(params, indexes, check_mode)?;
            if result.0 {
                changed = true;
                changes.push("indexes created".to_string());
            }
        }

        if let Some(validation_result) = apply_validation(params, check_mode)?
            && validation_result.0
        {
            changed = true;
            changes.push("validation updated".to_string());
        }

        if let Some(collation_result) = apply_collation(params, check_mode)?
            && collation_result.0
        {
            changed = true;
            changes.push("collation updated".to_string());
        }

        let extra = Some(value::to_value(json!({
            "collection": params.name,
            "database": params.database,
            "state": params.state.to_string(),
            "changes": changes,
        }))?);

        let msg = if changed {
            format!(
                "Collection '{}.{}' updated ({})",
                params.database,
                params.name,
                changes.join(", ")
            )
        } else {
            format!(
                "Collection '{}.{}' already exists",
                params.database, params.name
            )
        };

        return Ok(ModuleResult::new(changed, extra, Some(msg)));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!(
                "Would create collection '{}.{}'",
                params.database, params.name
            )),
        ));
    }

    let mut options_json = serde_json::Map::new();

    if let Some(ref collation) = params.collation {
        options_json.insert("collation".to_string(), hashmap_to_json_value(collation));
    }

    let has_validator = params.validator.is_some()
        || params.validation_level.is_some()
        || params.validation_action.is_some();

    if has_validator {
        let mut validation = serde_json::Map::new();
        if let Some(ref v) = params.validator {
            validation.insert("validator".to_string(), hashmap_to_json_value(v));
        }
        if let Some(ref level) = params.validation_level {
            validation.insert(
                "validationLevel".to_string(),
                serde_json::Value::String(level.to_string()),
            );
        }
        if let Some(ref action) = params.validation_action {
            validation.insert(
                "validationAction".to_string(),
                serde_json::Value::String(action.to_string()),
            );
        }
        options_json.insert(
            "validator".to_string(),
            serde_json::Value::Object(validation),
        );
    }

    let command = if options_json.is_empty() {
        format!("db.createCollection('{}')", params.name)
    } else {
        let options_str =
            serde_json::to_string(&serde_json::Value::Object(options_json)).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to serialize options: {}", e),
                )
            })?;
        format!("db.createCollection('{}', {})", params.name, options_str)
    };

    run_mongo_command(params, &command, &params.database)?;

    if let Some(ref indexes) = params.indexes {
        let _ = create_indexes_internal(params, indexes, check_mode)?;
    }

    let extra = Some(value::to_value(json!({
        "collection": params.name,
        "database": params.database,
        "state": params.state.to_string(),
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!(
            "Collection '{}.{}' created",
            params.database, params.name
        )),
    ))
}

fn drop_collection(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if !collection_exists(params)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!(
                "Collection '{}.{}' does not exist",
                params.database, params.name
            )),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!(
                "Would drop collection '{}.{}'",
                params.database, params.name
            )),
        ));
    }

    run_mongo_command(
        params,
        &format!("db.getCollection('{}').drop()", params.name),
        &params.database,
    )?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!(
            "Collection '{}.{}' dropped",
            params.database, params.name
        )),
    ))
}

fn create_indexes_internal(
    params: &Params,
    indexes: &[IndexParams],
    check_mode: bool,
) -> Result<(bool, Vec<String>)> {
    let existing_result = run_mongo_command(
        params,
        &format!(
            "db.getCollection('{}').getIndexes().map(i => i.name)",
            params.name
        ),
        &params.database,
    )?;

    let existing: Vec<String> = if existing_result.is_empty() || existing_result == "null" {
        vec!["_id_".to_string()]
    } else {
        serde_json::from_str(&existing_result).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse existing indexes: {}", e),
            )
        })?
    };

    let mut created_indexes = Vec::new();

    for index in indexes {
        let index_name = index.name.clone().unwrap_or_else(|| {
            let keys: Vec<String> = index
                .key
                .iter()
                .map(|(k, v)| format!("{}_{}", k, v))
                .collect();
            keys.join("_")
        });

        if existing.contains(&index_name) {
            continue;
        }

        if check_mode {
            created_indexes.push(index_name);
            continue;
        }

        let mut options = serde_json::Map::new();
        if index.unique {
            options.insert("unique".to_string(), serde_json::Value::Bool(true));
        }
        if index.sparse {
            options.insert("sparse".to_string(), serde_json::Value::Bool(true));
        }
        if index.background {
            options.insert("background".to_string(), serde_json::Value::Bool(true));
        }
        options.insert(
            "name".to_string(),
            serde_json::Value::String(index_name.clone()),
        );

        let keys_json = serde_json::Value::Object(
            index
                .key
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        let options_json = serde_json::Value::Object(options);

        let keys_str = serde_json::to_string(&keys_json).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to serialize keys: {}", e),
            )
        })?;
        let options_str = serde_json::to_string(&options_json).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to serialize options: {}", e),
            )
        })?;

        let command = format!(
            "db.getCollection('{}').createIndex({}, {})",
            params.name, keys_str, options_str
        );

        run_mongo_command(params, &command, &params.database)?;
        created_indexes.push(index_name);
    }

    Ok((!created_indexes.is_empty(), created_indexes))
}

fn apply_validation(params: &Params, check_mode: bool) -> Result<Option<(bool, String)>> {
    let has_validation = params.validator.is_some()
        || params.validation_level.is_some()
        || params.validation_action.is_some();

    if !has_validation {
        return Ok(None);
    }

    if check_mode {
        return Ok(Some((true, "validation would be updated".to_string())));
    }

    let mut coll_mod = serde_json::Map::new();

    if let Some(ref v) = params.validator {
        coll_mod.insert("validator".to_string(), hashmap_to_json_value(v));
    }
    if let Some(ref level) = params.validation_level {
        coll_mod.insert(
            "validationLevel".to_string(),
            serde_json::Value::String(level.to_string()),
        );
    }
    if let Some(ref action) = params.validation_action {
        coll_mod.insert(
            "validationAction".to_string(),
            serde_json::Value::String(action.to_string()),
        );
    }

    let cmd_json = serde_json::to_string(&serde_json::Value::Object(coll_mod)).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to serialize validation: {}", e),
        )
    })?;

    let command = format!(
        "db.runCommand({{ collMod: '{}', {} }})",
        params.name,
        cmd_json.trim_start_matches('{').trim_end_matches('}')
    );

    run_mongo_command(params, &command, &params.database)?;

    Ok(Some((true, "validation updated".to_string())))
}

fn apply_collation(params: &Params, check_mode: bool) -> Result<Option<(bool, String)>> {
    let collation = match params.collation.as_ref() {
        Some(c) => c,
        None => return Ok(None),
    };

    if check_mode {
        return Ok(Some((true, "collation would be updated".to_string())));
    }

    let collation_json = hashmap_to_json_value(collation);
    let collation_str = serde_json::to_string(&collation_json).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to serialize collation: {}", e),
        )
    })?;

    let command = format!(
        "db.runCommand({{ collMod: '{}', collation: {} }})",
        params.name, collation_str
    );

    run_mongo_command(params, &command, &params.database)?;

    Ok(Some((true, "collation updated".to_string())))
}

fn mongodb_collection_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Present => create_collection(&params, check_mode),
        State::Absent => drop_collection(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct MongodbCollection;

impl Module for MongodbCollection {
    fn get_name(&self) -> &str {
        "mongodb_collection"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mongodb_collection_impl(params, check_mode)?, None))
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
            name: users
            database: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "users");
        assert_eq!(params.database, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 27017);
        assert_eq!(params.auth_database, "admin");
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: users
            database: myapp
            state: present
            indexes:
              - key:
                  email: 1
                unique: true
                name: idx_email
              - key:
                  created_at: -1
                sparse: true
            validator:
              $jsonSchema:
                required:
                  - email
            validation_level: strict
            validation_action: error
            collation:
              locale: en
              strength: 2
            login_host: mongodb.example.com
            login_user: admin
            login_password: secret
            login_port: 27018
            replica_set: rs0
            auth_database: admin
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "users");
        assert_eq!(params.database, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.indexes.as_ref().unwrap().len(), 2);
        assert!(params.indexes.as_ref().unwrap()[0].unique);
        assert_eq!(
            params.indexes.as_ref().unwrap()[0].name,
            Some("idx_email".to_string())
        );
        assert_eq!(params.validation_level, Some(ValidationLevel::Strict));
        assert_eq!(params.validation_action, Some(ValidationAction::Error));
        assert!(params.validator.is_some());
        assert!(params.collation.is_some());
        assert_eq!(params.login_host, "mongodb.example.com");
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("secret".to_string()));
        assert_eq!(params.login_port, 27018);
        assert_eq!(params.replica_set, Some("rs0".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: logs
            database: myapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "logs");
        assert_eq!(params.database, "myapp");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_indexes() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: users
            database: myapp
            indexes:
              - key:
                  email: 1
                unique: true
              - key:
                  username: 1
                unique: true
                sparse: true
                name: idx_username
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let indexes = params.indexes.unwrap();
        assert_eq!(indexes.len(), 2);
        assert!(indexes[0].unique);
        assert!(!indexes[0].sparse);
        assert!(indexes[1].unique);
        assert!(indexes[1].sparse);
        assert_eq!(indexes[1].name, Some("idx_username".to_string()));
    }

    #[test]
    fn test_parse_params_validation() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: users
            database: myapp
            validator:
              email:
                $type: string
            validation_level: moderate
            validation_action: warn
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.validator.is_some());
        assert_eq!(params.validation_level, Some(ValidationLevel::Moderate));
        assert_eq!(params.validation_action, Some(ValidationAction::Warn));
    }

    #[test]
    fn test_build_mongo_uri_basic() {
        let params = Params {
            name: "users".to_string(),
            database: "myapp".to_string(),
            state: State::Present,
            indexes: None,
            validator: None,
            validation_level: None,
            validation_action: None,
            collation: None,
            replica_set: None,
            login_host: "localhost".to_string(),
            login_user: None,
            login_password: None,
            login_port: 27017,
            connection_options: None,
            auth_database: "admin".to_string(),
        };
        let uri = build_mongo_uri(&params);
        assert_eq!(uri, "mongodb://localhost:27017/admin");
    }

    #[test]
    fn test_build_mongo_uri_with_auth() {
        let params = Params {
            name: "users".to_string(),
            database: "myapp".to_string(),
            state: State::Present,
            indexes: None,
            validator: None,
            validation_level: None,
            validation_action: None,
            collation: None,
            replica_set: None,
            login_host: "mongodb.example.com".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 27018,
            connection_options: None,
            auth_database: "admin".to_string(),
        };
        let uri = build_mongo_uri(&params);
        assert_eq!(
            uri,
            "mongodb://admin:secret@mongodb.example.com:27018/admin"
        );
    }

    #[test]
    fn test_build_mongo_uri_with_replica_set() {
        let params = Params {
            name: "users".to_string(),
            database: "myapp".to_string(),
            state: State::Present,
            indexes: None,
            validator: None,
            validation_level: None,
            validation_action: None,
            collation: None,
            replica_set: Some("rs0".to_string()),
            login_host: "mongodb.example.com".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 27017,
            connection_options: None,
            auth_database: "admin".to_string(),
        };
        let uri = build_mongo_uri(&params);
        assert_eq!(
            uri,
            "mongodb://admin:secret@mongodb.example.com:27017/admin?replicaSet=rs0"
        );
    }

    #[test]
    fn test_build_mongo_uri_with_options() {
        let params = Params {
            name: "users".to_string(),
            database: "myapp".to_string(),
            state: State::Present,
            indexes: None,
            validator: None,
            validation_level: None,
            validation_action: None,
            collation: None,
            replica_set: None,
            login_host: "localhost".to_string(),
            login_user: None,
            login_password: None,
            login_port: 27017,
            connection_options: Some("readPreference=secondary".to_string()),
            auth_database: "admin".to_string(),
        };
        let uri = build_mongo_uri(&params);
        assert_eq!(
            uri,
            "mongodb://localhost:27017/admin?readPreference=secondary"
        );
    }

    #[test]
    fn test_state_display() {
        assert_eq!(State::Present.to_string(), "present");
        assert_eq!(State::Absent.to_string(), "absent");
    }

    #[test]
    fn test_validation_level_display() {
        assert_eq!(ValidationLevel::Off.to_string(), "off");
        assert_eq!(ValidationLevel::Strict.to_string(), "strict");
        assert_eq!(ValidationLevel::Moderate.to_string(), "moderate");
    }

    #[test]
    fn test_validation_action_display() {
        assert_eq!(ValidationAction::Error.to_string(), "error");
        assert_eq!(ValidationAction::Warn.to_string(), "warn");
    }

    #[test]
    fn test_parse_params_collation() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: users
            database: myapp
            collation:
              locale: en
              strength: 2
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.collation.is_some());
        let collation = params.collation.unwrap();
        assert!(collation.contains_key("locale"));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: users
            database: myapp
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_index_map_keys() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: users
            database: myapp
            indexes:
              - key:
                  email: 1
                  username: -1
                unique: true
                name: idx_email_username
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let index = &params.indexes.unwrap()[0];
        assert_eq!(index.key.len(), 2);
        assert!(index.key.contains_key("email"));
        assert!(index.key.contains_key("username"));
    }
}
