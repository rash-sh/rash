use crate::context::{Context, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult};
use crate::task::parse_file;
use crate::vars::builtin::Builtins;

use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use minijinja::{Value, context};
#[cfg(feature = "docs")]
use schemars::Schema;
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ParamType {
    #[default]
    String,
    Number,
    Object,
    Array,
    Boolean,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ParamDef {
    #[serde(rename = "type")]
    pub param_type: ParamType,
    #[serde(default)]
    pub required: bool,
    pub description: Option<String>,
    pub default: Option<YamlValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ModuleMeta {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, ParamDef>,
}

#[derive(Debug, Clone)]
pub struct DynamicModule {
    name: String,
    meta: ModuleMeta,
    main_path: PathBuf,
}

impl DynamicModule {
    pub fn load(module_dir: &Path) -> Result<Self> {
        let meta_path = module_dir.join("meta.yml");
        let main_path = module_dir.join("main.yml");

        if !meta_path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("meta.yml not found in {:?}", module_dir),
            ));
        }

        if !main_path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("main.yml not found in {:?}", module_dir),
            ));
        }

        let meta_content = read_to_string(&meta_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error reading meta.yml: {:?}", e),
            )
        })?;

        let meta: ModuleMeta = serde_norway::from_str(&meta_content).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error parsing meta.yml: {:?}", e),
            )
        })?;

        let name = module_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid module directory name: {:?}", module_dir),
                )
            })?
            .to_owned();

        Ok(DynamicModule {
            name,
            meta,
            main_path,
        })
    }

    pub fn get_name_str(&self) -> &str {
        &self.name
    }

    fn validate_params(&self, params: &YamlValue) -> Result<HashMap<String, YamlValue>> {
        let mut validated = HashMap::new();

        let params_map = match params {
            YamlValue::Mapping(m) => m.clone(),
            YamlValue::Null => serde_norway::Mapping::new(),
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Module params must be a mapping or null",
                ));
            }
        };

        for (param_name, param_def) in &self.meta.params {
            let value = params_map.get(YamlValue::String(param_name.clone()));

            match value {
                Some(v) => {
                    validated.insert(param_name.clone(), v.clone());
                }
                None => {
                    if param_def.required {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!(
                                "Required parameter '{}' missing for module '{}'",
                                param_name, self.name
                            ),
                        ));
                    } else if let Some(default) = &param_def.default {
                        validated.insert(param_name.clone(), default.clone());
                    }
                }
            }
        }

        for key in params_map.keys() {
            if let YamlValue::String(key_str) = key
                && !self.meta.params.contains_key(key_str)
            {
                trace!(
                    "Unknown parameter '{}' passed to module '{}', ignoring",
                    key_str, self.name
                );
            }
        }

        Ok(validated)
    }

    fn convert_to_value(params: HashMap<String, YamlValue>) -> Value {
        Value::from_serialize(
            params
                .into_iter()
                .map(|(k, v)| (k, yaml_to_json(v)))
                .collect::<HashMap<String, serde_json::Value>>(),
        )
    }
}

fn yaml_to_json(value: YamlValue) -> serde_json::Value {
    match value {
        YamlValue::Null => serde_json::Value::Null,
        YamlValue::Bool(b) => serde_json::Value::Bool(b),
        YamlValue::Number(n) => serde_json::Value::Number(n.as_i64().map_or_else(
            || {
                n.as_f64()
                    .map(|f| {
                        serde_json::Number::from_f64(f)
                            .unwrap_or_else(|| serde_json::Number::from(0))
                    })
                    .unwrap_or_else(|| serde_json::Number::from(0))
            },
            serde_json::Number::from,
        )),
        YamlValue::String(s) => serde_json::Value::String(s),
        YamlValue::Sequence(seq) => {
            serde_json::Value::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        YamlValue::Mapping(map) => serde_json::Value::Object(
            map.into_iter()
                .filter_map(|(k, v)| {
                    if let YamlValue::String(key) = k {
                        Some((key, yaml_to_json(v)))
                    } else {
                        None
                    }
                })
                .collect(),
        ),
        YamlValue::Tagged(tagged) => yaml_to_json(tagged.value.clone()),
    }
}

impl Module for DynamicModule {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn exec(
        &self,
        global_params: &GlobalParams,
        params: YamlValue,
        vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let validated_params = self.validate_params(&params)?;
        let params_value = Self::convert_to_value(validated_params);

        let module_vars = context! {
            module => context! {
                name => self.name.clone(),
                params => params_value,
                check_mode => check_mode,
            },
        };

        let exec_vars = context! { ..module_vars, ..vars.clone() };

        trace!("Dynamic module '{}' vars: {:?}", self.name, exec_vars);

        let main_content = read_to_string(&self.main_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error reading main.yml for module '{}': {:?}", self.name, e),
            )
        })?;

        let tasks = parse_file(&main_content, global_params)?;

        let builtins = Builtins::deserialize(vars.get_attr("rash")?)?;
        let module_builtins = builtins.update(&self.main_path)?;
        let module_exec_vars = context! {rash => &module_builtins, ..exec_vars};

        let result_context = Context::new(tasks, module_exec_vars, None).exec()?;

        let result_vars = result_context.get_vars();

        let changed = result_vars
            .get_attr("__module_changed")
            .ok()
            .map(|v| {
                serde_json::to_value(&v)
                    .ok()
                    .and_then(|j| j.as_bool())
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        let output = result_vars
            .get_attr("__module_output")
            .ok()
            .and_then(|v| v.as_str().map(String::from));

        let extra = result_vars.get_attr("__module_extra").ok().map(|v| {
            let json_str = serde_json::to_string(&v).unwrap_or_default();
            serde_norway::from_str(&json_str).unwrap_or(YamlValue::Null)
        });

        let new_vars = if changed || output.is_some() || extra.is_some() {
            let mut result_map = serde_json::Map::new();
            if let Some(o) = &output {
                result_map.insert("output".to_string(), serde_json::Value::String(o.clone()));
            }
            if let Some(e) = &extra
                && let Ok(json_val) = serde_json::to_value(e)
            {
                result_map.insert("extra".to_string(), json_val);
            }
            Some(Value::from_serialize(result_map))
        } else {
            None
        };

        Ok((ModuleResult::new(changed, extra, output), new_vars))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        None
    }
}

pub struct DynamicModuleRegistry {
    modules: HashMap<String, DynamicModule>,
    search_paths: Vec<PathBuf>,
}

impl DynamicModuleRegistry {
    pub fn new() -> Self {
        DynamicModuleRegistry {
            modules: HashMap::new(),
            search_paths: Vec::new(),
        }
    }

    pub fn with_search_paths(search_paths: Vec<PathBuf>) -> Self {
        DynamicModuleRegistry {
            modules: HashMap::new(),
            search_paths,
        }
    }

    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    pub fn get(&self, name: &str) -> Option<&DynamicModule> {
        self.modules.get(name)
    }

    pub fn load_module(&mut self, name: &str) -> Result<&DynamicModule> {
        if self.modules.contains_key(name) {
            return Ok(self.modules.get(name).unwrap());
        }

        for search_path in &self.search_paths {
            let module_dir = search_path.join(name);
            if module_dir.exists() && module_dir.is_dir() {
                let module = DynamicModule::load(&module_dir)?;
                let module_name = module.name.clone();
                self.modules.insert(module_name, module);
                return Ok(self.modules.get(name).unwrap());
            }
        }

        Err(Error::new(
            ErrorKind::NotFound,
            format!("Dynamic module '{}' not found in search paths", name),
        ))
    }

    pub fn is_dynamic_module(&mut self, name: &str) -> bool {
        if self.modules.contains_key(name) {
            return true;
        }

        for search_path in &self.search_paths {
            let module_dir = search_path.join(name);
            if module_dir.join("meta.yml").exists() && module_dir.join("main.yml").exists() {
                return true;
            }
        }

        false
    }

    pub fn load_all(&mut self) -> Result<()> {
        for search_path in &self.search_paths {
            if search_path.exists() && search_path.is_dir() {
                let entries = std::fs::read_dir(search_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Error reading modules directory: {:?}", e),
                    )
                })?;

                for entry in entries {
                    let entry = entry.map_err(|e| {
                        Error::new(
                            ErrorKind::InvalidData,
                            format!("Error reading directory entry: {:?}", e),
                        )
                    })?;

                    let path = entry.path();
                    if path.is_dir() {
                        let meta_path = path.join("meta.yml");
                        let main_path = path.join("main.yml");

                        if meta_path.exists()
                            && main_path.exists()
                            && let Ok(module) = DynamicModule::load(&path)
                        {
                            let name = module.name.clone();
                            self.modules.entry(name).or_insert(module);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for DynamicModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_module(temp_dir: &TempDir, name: &str) -> PathBuf {
        let module_dir = temp_dir.path().join(name);
        std::fs::create_dir_all(&module_dir).unwrap();

        let meta_content = r#"
name: test_module
description: A test module
params:
  message:
    type: string
    required: true
    description: The message to display
  count:
    type: number
    required: false
    default: 1
"#;
        let mut meta_file = std::fs::File::create(module_dir.join("meta.yml")).unwrap();
        meta_file.write_all(meta_content.as_bytes()).unwrap();

        let main_content = r#"
- name: Set default changed
  set_vars:
    __module_changed: false

- name: Display message
  debug:
    msg: "{{ module.params.message }}"
"#;
        let mut main_file = std::fs::File::create(module_dir.join("main.yml")).unwrap();
        main_file.write_all(main_content.as_bytes()).unwrap();

        module_dir
    }

    #[test]
    fn test_load_dynamic_module() {
        let temp_dir = TempDir::new().unwrap();
        create_test_module(&temp_dir, "test_module");

        let module_dir = temp_dir.path().join("test_module");
        let module = DynamicModule::load(&module_dir).unwrap();

        assert_eq!(module.name, "test_module");
        assert_eq!(module.meta.description, Some("A test module".to_string()));
        assert!(module.meta.params.contains_key("message"));
        assert!(module.meta.params.contains_key("count"));
    }

    #[test]
    fn test_validate_params_required() {
        let temp_dir = TempDir::new().unwrap();
        create_test_module(&temp_dir, "test_module");

        let module_dir = temp_dir.path().join("test_module");
        let module = DynamicModule::load(&module_dir).unwrap();

        let params = YamlValue::Mapping(serde_norway::Mapping::new());
        let result = module.validate_params(&params);
        assert!(result.is_err());

        let params = YamlValue::Mapping({
            let mut m = serde_norway::Mapping::new();
            m.insert(
                YamlValue::String("message".to_string()),
                YamlValue::String("hello".to_string()),
            );
            m
        });
        let result = module.validate_params(&params).unwrap();
        assert!(result.contains_key("message"));
        assert!(result.contains_key("count"));
        assert_eq!(result["count"], YamlValue::Number(1.into()));
    }

    #[test]
    fn test_dynamic_module_registry() {
        let temp_dir = TempDir::new().unwrap();
        create_test_module(&temp_dir, "test_module");

        let mut registry =
            DynamicModuleRegistry::with_search_paths(vec![temp_dir.path().to_path_buf()]);

        assert!(registry.is_dynamic_module("test_module"));
        assert!(!registry.is_dynamic_module("non_existent"));

        let module = registry.load_module("test_module").unwrap();
        assert_eq!(module.name, "test_module");
    }
}
