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
/// ANCHOR: parameters
/// `include` takes either the file path or a mapping:
///
/// | Parameter | Required | Type            | Values | Description                                                              |
/// | --------- | -------- | --------------- | ------ | ------------------------------------------------------------------------ |
/// | file      | true     | string          |        | Rash file whose tasks are executed with the caller's variables.          |
/// | export    | false    | boolean or list |        | Return all (`true`) or the listed variables set by the file to the caller. |
///
/// ANCHOR_END: parameters
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
use crate::task::parse_script;
use crate::vars::builtin::Builtins;

use std::collections::BTreeMap;
use std::fs::read_to_string;
use std::path::Path;

use minijinja::{Value, context};
use minijinja::value::Serde;
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
    match export {
        Export::All(false) => Ok(None),
        Export::All(true) => Ok(scoped.cloned()),
        Export::Selected(names) => {
            let mut exported_map: BTreeMap<&str, Value> = BTreeMap::new();
            for name in names {
                // A file that set no variables has no scope: nothing is defined.
                let value = scoped
                    .and_then(|scoped| scoped.get_attr(name).ok())
                    .filter(|value| !value.is_undefined())
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::NotFound,
                            format!("Included file did not define exported variable '{name}'"),
                        )
                    })?;
                exported_map.insert(name, value);
            }
            Ok(Some(Value::from(Serde(exported_map))))
        }
    }
}

#[derive(Debug)]
pub struct Include;

impl Module for Include {
    fn get_name(&self) -> &str {
        "include"
    }

    fn is_control_flow(&self) -> bool {
        true
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
            Error::new(
                ErrorKind::InvalidData,
                format!("Error reading file {}: {e}", params.file),
            )
        })?;

        let builtins = Builtins::deserialize(vars.get_attr("rash")?)?;
        let include_builtins = builtins.update(script_path)?;
        let include_vars = context! {rash => Serde(&include_builtins), ..vars.clone()};

        let parsed = parse_script(&main_file, global_params)?;
        let result_context =
            Context::with_handlers(parsed.tasks, include_vars, None, parsed.handlers).exec()?;

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
    fn select_named_exports_rejects_missing_scope() {
        let error = select_exports(None, Some(&Export::Selected(vec!["foo".into()]))).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("did not define exported variable 'foo'")
        );
        assert!(
            select_exports(None, Some(&Export::All(true)))
                .unwrap()
                .is_none()
        );
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
