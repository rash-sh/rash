/// ANCHOR: module
/// # mongodb_db
///
/// Manage MongoDB databases.
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
/// - name: Create a new database with name "myapp"
///   mongodb_db:
///     name: myapp
///     state: present
///
/// - name: Create database with specific credentials
///   mongodb_db:
///     name: myapp
///     state: present
///     login_user: admin
///     login_password: secret
///     login_host: mongodb.example.com
///     login_port: 27017
///
/// - name: Create a collection in database
///   mongodb_db:
///     name: myapp
///     collection: users
///     state: present
///
/// - name: Create indexes on a collection
///   mongodb_db:
///     name: myapp
///     collection: users
///     indexes:
///       - key: email
///         unique: true
///       - key: created_at
///         name: idx_created_at
///
/// - name: Drop a collection
///   mongodb_db:
///     name: myapp
///     collection: old_data
///     state: absent
///
/// - name: Drop database
///   mongodb_db:
///     name: oldapp
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, Module, ModuleResult};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::value;
use serde_norway::Value as YamlValue;
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
#[serde(deny_unknown_fields)]
pub struct IndexParams {
    /// Key field(s) for the index.
    pub key: String,
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
    /// Name of the database to manage.
    pub name: String,
    /// The database/collection state.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// Collection name to manage within the database.
    pub collection: Option<String>,
    /// List of indexes to create on the collection.
    pub indexes: Option<Vec<IndexParams>>,
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
        command,
        database
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

fn database_exists(params: &Params) -> Result<bool> {
    let result = run_mongo_command(
        params,
        &format!("db.getMongo().getDBNames().indexOf('{}') >= 0", params.name),
        "admin",
    )?;

    Ok(result == "true")
}

fn collection_exists(params: &Params) -> Result<bool> {
    let collection = params
        .collection
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "collection is required"))?;

    let result = run_mongo_command(
        params,
        &format!("db.getCollectionNames().indexOf('{}') >= 0", collection),
        &params.name,
    )?;

    Ok(result == "true")
}

fn create_database(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if database_exists(params)? {
        let extra = Some(value::to_value(json!({
            "db": params.name,
            "state": params.state.to_string(),
        }))?);

        return Ok(ModuleResult::new(
            false,
            extra,
            Some(format!("Database '{}' already exists", params.name)),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create database '{}'", params.name)),
        ));
    }

    run_mongo_command(
        params,
        "db.createCollection('init_collection')",
        &params.name,
    )?;
    run_mongo_command(params, "db.init_collection.drop()", &params.name)?;

    let extra = Some(value::to_value(json!({
        "db": params.name,
        "state": params.state.to_string(),
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("Database '{}' created", params.name)),
    ))
}

fn drop_database(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if !database_exists(params)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!("Database '{}' does not exist", params.name)),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would drop database '{}'", params.name)),
        ));
    }

    run_mongo_command(params, "db.dropDatabase()", &params.name)?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Database '{}' dropped", params.name)),
    ))
}

fn create_collection(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let collection = params
        .collection
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "collection is required"))?;

    if !database_exists(params)? {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Database '{}' does not exist", params.name),
        ));
    }

    if collection_exists(params)? {
        let extra = Some(value::to_value(json!({
            "db": params.name,
            "collection": collection,
            "state": params.state.to_string(),
        }))?);

        return Ok(ModuleResult::new(
            false,
            extra,
            Some(format!(
                "Collection '{}' already exists in database '{}'",
                collection, params.name
            )),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!(
                "Would create collection '{}' in database '{}'",
                collection, params.name
            )),
        ));
    }

    run_mongo_command(
        params,
        &format!("db.createCollection('{}')", collection),
        &params.name,
    )?;

    let extra = Some(value::to_value(json!({
        "db": params.name,
        "collection": collection,
        "state": params.state.to_string(),
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!(
            "Collection '{}' created in database '{}'",
            collection, params.name
        )),
    ))
}

fn drop_collection(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let collection = params
        .collection
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "collection is required"))?;

    if !database_exists(params)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!("Database '{}' does not exist", params.name)),
        ));
    }

    if !collection_exists(params)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!(
                "Collection '{}' does not exist in database '{}'",
                collection, params.name
            )),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!(
                "Would drop collection '{}' from database '{}'",
                collection, params.name
            )),
        ));
    }

    run_mongo_command(
        params,
        &format!("db.getCollection('{}').drop()", collection),
        &params.name,
    )?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!(
            "Collection '{}' dropped from database '{}'",
            collection, params.name
        )),
    ))
}

fn get_existing_indexes(params: &Params) -> Result<Vec<String>> {
    let collection = params
        .collection
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "collection is required"))?;

    let result = run_mongo_command(
        params,
        &format!(
            "db.getCollection('{}').getIndexes().map(i => i.name)",
            collection
        ),
        &params.name,
    )?;

    let indexes: Vec<String> = serde_json::from_str(&result).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse indexes: {}", e),
        )
    })?;

    Ok(indexes)
}

fn create_indexes(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let collection = params
        .collection
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "collection is required for indexes"))?;

    let indexes = params
        .indexes
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "indexes is required"))?;

    if !database_exists(params)? {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Database '{}' does not exist", params.name),
        ));
    }

    if !collection_exists(params)? {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Collection '{}' does not exist in database '{}'",
                collection, params.name
            ),
        ));
    }

    let existing = get_existing_indexes(params)?;
    let mut created_indexes = Vec::new();

    for index in indexes {
        let index_name = index.name.as_ref().unwrap_or(&index.key);

        if existing.contains(index_name) {
            continue;
        }

        if check_mode {
            created_indexes.push(index_name.clone());
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
        if let Some(ref name) = index.name {
            options.insert("name".to_string(), serde_json::Value::String(name.clone()));
        }

        let keys_json = serde_json::json!({ &index.key: 1 });
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
            collection, keys_str, options_str
        );

        run_mongo_command(params, &command, &params.name)?;
        created_indexes.push(index_name.clone());
    }

    if created_indexes.is_empty() {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!(
                "All indexes already exist on collection '{}'",
                collection
            )),
        ));
    }

    let extra = Some(value::to_value(json!({
        "db": params.name,
        "collection": collection,
        "indexes": created_indexes,
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!(
            "Indexes {} created on collection '{}'",
            created_indexes.join(", "),
            collection
        )),
    ))
}

fn mongodb_db_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Present => {
            if params.indexes.is_some() {
                create_indexes(&params, check_mode)
            } else if params.collection.is_some() {
                create_collection(&params, check_mode)
            } else {
                create_database(&params, check_mode)
            }
        }
        State::Absent => {
            if params.collection.is_some() {
                drop_collection(&params, check_mode)
            } else {
                drop_database(&params, check_mode)
            }
        }
    }
}

#[derive(Debug)]
pub struct MongodbDb;

impl Module for MongodbDb {
    fn get_name(&self) -> &str {
        "mongodb_db"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mongodb_db_impl(params, check_mode)?, None))
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
            name: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 27017);
        assert_eq!(params.auth_database, "admin");
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: present
            collection: users
            indexes:
              - key: email
                unique: true
                name: idx_email
              - key: created_at
                sparse: true
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
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.collection, Some("users".to_string()));
        assert_eq!(params.indexes.unwrap().len(), 2);
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
            name: oldapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "oldapp");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_collection_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            collection: old_data
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.collection, Some("old_data".to_string()));
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_index() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            collection: users
            indexes:
              - key: email
                unique: true
              - key: username
                unique: true
                sparse: true
                name: idx_username
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let indexes = params.indexes.unwrap();
        assert_eq!(indexes.len(), 2);
        assert_eq!(indexes[0].key, "email");
        assert!(indexes[0].unique);
        assert!(!indexes[0].sparse);
        assert_eq!(indexes[1].key, "username");
        assert!(indexes[1].unique);
        assert!(indexes[1].sparse);
        assert_eq!(indexes[1].name, Some("idx_username".to_string()));
    }

    #[test]
    fn test_build_mongo_uri_basic() {
        let params = Params {
            name: "myapp".to_string(),
            state: State::Present,
            collection: None,
            indexes: None,
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
            name: "myapp".to_string(),
            state: State::Present,
            collection: None,
            indexes: None,
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
            name: "myapp".to_string(),
            state: State::Present,
            collection: None,
            indexes: None,
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
            name: "myapp".to_string(),
            state: State::Present,
            collection: None,
            indexes: None,
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
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
