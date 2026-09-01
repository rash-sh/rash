/// ANCHOR: module
/// # include
///
/// Include and execute tasks from another Rash file. Included tasks receive the caller context.
/// By default variables created by the include remain scoped; `export` can explicitly return all
/// or selected variables to the caller.
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
/// - include: foo.rh
///
/// - include:
///     file: "{{ rash.dir }}/detect.rh"
///     export: true
///
/// - include:
///     file: "{{ rash.dir }}/build.rh"
///     export:
///       - artifact
///       - checksum
/// ```
/// ANCHOR_END: examples
use crate::context::{Context, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};
use crate::task::{parse_file, parse_file_with_handlers};
use crate::vars::builtin::Builtins;

use std::fs::read_to_string;
use std::path::Path;

use minijinja::{Value, context};
#[cfg(feature = "docs")]
use schemars::Schema;
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum Export {
    All(bool),
    Selected(Vec<String>),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Params {
    file: String,
    #[serde(default)]
    export: Option<Export>,
}

fn select_exports(scoped: Option<&Value>, export: Option<&Export>) -> Result<Option<Value>> {
    let Some(export) = export else {
        return Ok(None);
    };
    let Some(scoped) = scoped else {
        return Ok(None);
    };

    match export {
        Export::All(false) => Ok(None),
        Export::All(true) => Ok(Some(scoped.clone())),
        Export::Selected(names) => {
            use std::collections::BTreeMap;
            let mut exported_map: BTreeMap<&str, Value> = BTreeMap::new();
            for name in names {
                let value = scoped.get_attr(name).map_err(|_| {
                    Error::new(
                        ErrorKind::NotFound,
                        format!("Included file did not define exported variable '{name}'"),
                    )
                })?;
                if value.is_undefined() {
                    return Err(Error::new(
                        ErrorKind::NotFound,
                        format!("Included file did not define exported variable '{name}'"),
                    ));
                }
                exported_map.insert(name, value);
            }
            Ok(Some(Value::from_serialize(exported_map)))
        }
    }
}

#[derive(Debug)]
pub struct Include;

impl Module for Include {
    fn get_name(&self) -> &str {
        "include"
    }

    fn exec(
        &self,
        global_params: &GlobalParams,
        params: YamlValue,
        vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params = match params.as_str() {
            Some(file) => Params {
                file: file.to_owned(),
                export: None,
            },
            None => parse_params(params)?,
        };

        let script_path = Path::new(&params.file);
        trace!("reading tasks from: {script_path:?}");
        let main_file = read_to_string(script_path).map_err(|e| {
            Error::new(ErrorKind::InvalidData, format!("Error reading file: {e:?}"))
        })?;

        let builtins = Builtins::deserialize(vars.get_attr("rash")?)?;
        let include_builtins = builtins.update(script_path)?;
        let include_vars = context! {rash => &include_builtins, ..vars.clone()};

        let result_context = match parse_file_with_handlers(&main_file, global_params) {
            Ok(parsed) => {
                Context::with_handlers(parsed.tasks, include_vars, None, parsed.handlers).exec()?
            }
            Err(_) => {
                Context::new(parse_file(&main_file, global_params)?, include_vars, None).exec()?
            }
        };

        let exports = select_exports(result_context.get_scoped_vars(), params.export.as_ref())?;
        Ok((ModuleResult::new(false, None, None), exports))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scalar_is_backward_compatible() {
        let params = Params {
            file: "foo.rh".to_owned(),
            export: None,
        };
        assert_eq!(params.file, "foo.rh");
    }

    #[test]
    fn parse_mapping_with_selected_exports() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            file: foo.rh
            export: [artifact, checksum]
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.export,
            Some(Export::Selected(vec!["artifact".into(), "checksum".into()]))
        );
    }

    #[test]
    fn select_all_exports_scope() {
        let scope = context! {foo => 1, bar => "two"};
        let exported = select_exports(Some(&scope), Some(&Export::All(true)))
            .unwrap()
            .unwrap();
        assert_eq!(exported.get_attr("foo").unwrap().as_i64(), Some(1));
    }

    #[test]
    fn select_named_exports_rejects_missing_values() {
        let scope = context! {foo => 1};
        let result = select_exports(
            Some(&scope),
            Some(&Export::Selected(vec!["missing".into()])),
        );
        assert!(result.is_err());
    }
}
