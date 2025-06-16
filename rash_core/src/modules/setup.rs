/// ANCHOR: module
/// # setup
///
/// Load variables from .env, YAML, and JSON files.
///
/// Environment variables from .env files are loaded into the `env` namespace, while
/// YAML and JSON variables are loaded as top-level context variables.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Load configuration from multiple sources
///   setup:
///     from:
///       - .env
///       - config.yaml
///       - settings.json
///
/// - name: Use loaded variables
///   debug:
///     msg: "Database URL: {{ env.DATABASE_URL }}"
///
/// - name: Load from single file
///   setup:
///     from: vars/production.yml
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize, Serialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// List of file paths to load variables from.
    /// Supports .env, .yaml/.yml, and .json files. `.env` files are loaded into the `env`
    /// namespace, while YAML and JSON files are loaded as top-level context variables.
    /// If a file has no extension, its format is auto-detected based on its content.
    #[serde(default)]
    from: Vec<String>,
}

fn load_file_vars_with_type(file_path: &str) -> Result<(serde_json::Value, bool)> {
    let path = Path::new(file_path);

    let content = read_to_string(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read file '{}': {}", file_path, e),
        )
    })?;

    detect_and_load_file_format(&content, path)
}

fn detect_and_load_file_format(content: &str, path: &Path) -> Result<(serde_json::Value, bool)> {
    match path.extension().and_then(|s| s.to_str()) {
        Some("env") => {
            let vars = load_env_vars(content)?;
            Ok((vars, true))
        }
        Some("yaml") | Some("yml") => {
            let vars = load_yaml_vars(content)?;
            Ok((vars, false))
        }
        Some("json") => {
            let vars = load_json_vars(content)?;
            Ok((vars, false))
        }
        _ => {
            // Auto-detect format by content or filename
            if path.file_name().and_then(|s| s.to_str()) == Some(".env") {
                let vars = load_env_vars(content)?;
                Ok((vars, true))
            } else if content.trim_start().starts_with('{') {
                let vars = load_json_vars(content)?;
                Ok((vars, false))
            } else if content.contains('=') && !content.trim_start().starts_with('-') {
                let vars = load_env_vars(content)?;
                Ok((vars, true))
            } else {
                let vars = load_yaml_vars(content)?;
                Ok((vars, false))
            }
        }
    }
}

fn load_env_vars(content: &str) -> Result<serde_json::Value> {
    let mut vars = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(pos) = line.find('=') {
            let key = line[..pos].trim();
            let value = line[pos + 1..].trim();

            // Validate environment variable name (POSIX: [a-zA-Z_][a-zA-Z0-9_]*)
            let mut chars = key.chars();
            let valid = match chars.next() {
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
                }
                _ => false,
            };
            if !valid {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Invalid environment variable name '{}' at line {}",
                        key,
                        line_num + 1
                    ),
                ));
            }

            // Remove quotes if present
            let cleaned_value = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                &value[1..value.len() - 1]
            } else {
                value
            };

            vars.insert(key.to_string(), cleaned_value.to_string());
        } else {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid .env format at line {}: missing '='", line_num + 1),
            ));
        }
    }

    serde_json::to_value(vars)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("serde_json error: {}", e)))
}

fn load_yaml_vars(content: &str) -> Result<serde_json::Value> {
    let yaml_value: YamlValue = serde_yaml::from_str(content)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid YAML: {}", e)))?;

    serde_json::to_value(yaml_value).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("YAML conversion error: {}", e),
        )
    })
}

fn load_json_vars(content: &str) -> Result<serde_json::Value> {
    serde_json::from_str(content)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid JSON: {}", e)))
}

fn merge_context_with_env_vars(
    context_json: &mut serde_json::Map<String, serde_json::Value>,
    env_vars: serde_json::Value,
) {
    if let serde_json::Value::Object(env_map) = env_vars {
        // Get or create env object
        let env_obj = context_json
            .entry("env".to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        if let serde_json::Value::Object(env_existing) = env_obj {
            // Merge .env variables into env object
            for (k, v) in env_map {
                env_existing.insert(k, v);
            }
        }
    }
}

fn merge_context_with_regular_vars(
    context_json: &mut serde_json::Map<String, serde_json::Value>,
    file_vars: serde_json::Value,
) {
    if let serde_json::Value::Object(new_map) = file_vars {
        // Merge all regular variables as top-level keys
        for (k, v) in new_map {
            context_json.insert(k, v);
        }
    }
}

fn load_and_merge_files(file_paths: &[String], context: Value) -> Result<(Vec<String>, Value)> {
    let mut loaded_files = Vec::with_capacity(file_paths.len());

    // Convert context to JSON for easier manipulation
    let mut context_json: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(serde_json::to_value(&context).unwrap_or_default())
            .unwrap_or_default();

    for file_path in file_paths {
        match load_file_vars_with_type(file_path) {
            Ok((file_vars, is_env_file)) => {
                if is_env_file {
                    merge_context_with_env_vars(&mut context_json, file_vars);
                } else {
                    merge_context_with_regular_vars(&mut context_json, file_vars);
                }

                loaded_files.push(file_path.clone());
            }
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to load '{}': {}", file_path, e),
                ));
            }
        }
    }

    // Convert back to minijinja Value
    let final_context = Value::from_serialize(context_json);
    Ok((loaded_files, final_context))
}

#[derive(Debug)]
pub struct Setup;

impl Module for Setup {
    fn get_name(&self) -> &str {
        "setup"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        vars: Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;

        if params.from.is_empty() {
            return Ok((
                ModuleResult::new(false, None, Some("No files specified to load".to_string())),
                None,
            ));
        }

        let (loaded_files, merged_vars) = load_and_merge_files(&params.from, vars.clone())?;

        // Calculate only the new variables that were loaded
        let new_variables = calculate_new_variables_setup(&vars, &merged_vars)?;

        Ok((
            ModuleResult::new(
                !loaded_files.is_empty(),
                None,
                Some(format!(
                    "Loaded variables from: {}",
                    loaded_files.join(", ")
                )),
            ),
            new_variables,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

/// Calculate the difference between original and merged variables
/// Returns only the new variables that were loaded from files
fn calculate_new_variables_setup(original: &Value, merged: &Value) -> Result<Option<Value>> {
    // Convert both values to JSON for easier comparison
    let original_json: serde_json::Value = serde_json::to_value(original).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Serialization error: {}", e),
        )
    })?;
    let merged_json: serde_json::Value = serde_json::to_value(merged).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Serialization error: {}", e),
        )
    })?;

    if original_json == merged_json {
        return Ok(None);
    }

    match (original_json, merged_json) {
        (serde_json::Value::Object(orig_map), serde_json::Value::Object(merged_map)) => {
            let mut new_vars = serde_json::Map::new();

            for (key, value) in merged_map {
                // Include new keys or keys with different values
                if !orig_map.contains_key(&key) || orig_map.get(&key) != Some(&value) {
                    new_vars.insert(key, value);
                }
            }

            if new_vars.is_empty() {
                Ok(None)
            } else {
                let result = Value::from_serialize(serde_json::Value::Object(new_vars));
                Ok(Some(result))
            }
        }
        _ => {
            // If structure changed completely, return the merged result
            Ok(Some(merged.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            from:
              - .env
              - config.yaml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                from: vec![".env".to_owned(), "config.yaml".to_owned()],
            }
        );
    }

    #[test]
    fn test_parse_params_single_file() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            from:
              - config.json
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                from: vec!["config.json".to_owned()],
            }
        );
    }

    #[test]
    fn test_parse_params_empty() {
        let yaml: YamlValue = serde_yaml::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params, Params { from: vec![] });
    }

    #[test]
    fn test_load_env_vars() {
        let content = r#"
# This is a comment
DATABASE_URL=postgres://localhost/mydb
API_KEY="secret-key"
DEBUG=true
PORT=3000
EMPTY_VAR=
        "#;

        let result = load_env_vars(content).unwrap();
        let expected = serde_json::json!({
            "DATABASE_URL": "postgres://localhost/mydb",
            "API_KEY": "secret-key",
            "DEBUG": "true",
            "PORT": "3000",
            "EMPTY_VAR": ""
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_load_yaml_vars() {
        let content = r#"
database:
  host: localhost
  port: 5432
  name: mydb
api:
  key: secret
  timeout: 30
        "#;

        let result = load_yaml_vars(content).unwrap();
        let expected = serde_json::json!({
            "database": {
                "host": "localhost",
                "port": 5432,
                "name": "mydb"
            },
            "api": {
                "key": "secret",
                "timeout": 30
            }
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_load_json_vars() {
        let content = r#"
{
    "app": {
        "name": "myapp",
        "version": "1.0.0"
    },
    "features": ["auth", "api"]
}
        "#;

        let result = load_json_vars(content).unwrap();
        let expected = serde_json::json!({
            "app": {
                "name": "myapp",
                "version": "1.0.0"
            },
            "features": ["auth", "api"]
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_setup_context_no_files() {
        let setup = Setup;
        let params = Params { from: vec![] };
        let vars = context! { existing => "value" };
        let yaml_params = serde_yaml::to_value(&params).unwrap();

        let (result, new_vars) = setup
            .exec(&GlobalParams::default(), yaml_params, vars.clone(), false)
            .unwrap();

        assert!(!result.get_changed());
        assert!(result.get_output().unwrap().contains("No files specified"));
        assert_eq!(new_vars, None);
    }

    #[test]
    fn test_setup_context_with_files() {
        // Create temporary files
        let mut env_file = NamedTempFile::new().unwrap();
        writeln!(env_file, "TEST_VAR=hello").unwrap();
        writeln!(env_file, "PORT=8080").unwrap();
        env_file.flush().unwrap(); // Ensure data is written to disk

        let mut yaml_file = NamedTempFile::new().unwrap();
        writeln!(yaml_file, "config:").unwrap();
        writeln!(yaml_file, "  debug: true").unwrap();
        yaml_file.flush().unwrap(); // Ensure data is written to disk

        let setup = Setup;
        let params = Params {
            from: vec![
                env_file.path().to_str().unwrap().to_string(),
                yaml_file.path().to_str().unwrap().to_string(),
            ],
        };
        let vars = context! { existing => "value", env => context! {} };
        let yaml_params = serde_yaml::to_value(&params).unwrap();

        let (result, new_vars) = setup
            .exec(&GlobalParams::default(), yaml_params, vars.clone(), false)
            .unwrap();

        assert!(result.get_changed());
        assert!(
            result
                .get_output()
                .unwrap()
                .contains("Loaded variables from")
        );

        // Check that new variables were returned
        assert!(new_vars.is_some());
        let new_vars = new_vars.unwrap();

        // Check env variables were loaded
        assert_eq!(
            new_vars
                .get_attr("env")
                .unwrap()
                .get_attr("TEST_VAR")
                .unwrap()
                .to_string(),
            "hello"
        );
        assert_eq!(
            new_vars
                .get_attr("env")
                .unwrap()
                .get_attr("PORT")
                .unwrap()
                .to_string(),
            "8080"
        );

        // Check that config was loaded as top-level variable
        assert!(new_vars.get_attr("config").is_ok());
    }

    #[test]
    fn test_load_file_security_validation() {
        // Test non-existent file
        let result = load_file_vars_with_type("/non/existent/file.env");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_env_vars_validation() {
        // Test invalid variable name
        let content = "123INVALID=value\nVALID_VAR=test";
        let result = load_env_vars(content);
        assert!(result.is_err());

        // Test missing equals sign
        let content = "INVALID_LINE_WITHOUT_EQUALS";
        let result = load_env_vars(content);
        assert!(result.is_err());

        // Test empty key
        let content = "=value";
        let result = load_env_vars(content);
        assert!(result.is_err());
    }
}
