/// ANCHOR: module
/// # set_vars
///
/// This module allows setting new variables.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: parameters
/// | Parameter | Required | Type  | Values | Description                                                         |
/// |-----------|----------|-------|--------|---------------------------------------------------------------------|
/// | key_value | true     | map   |        | This module takes key/value pairs and save un current Vars context. |
///
/// ANCHOR_END: parameters
///
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - set_vars:
///     foo: boo
///     buu:
///       - 1
///       - 2
///       - 3
///     zoo:
///       suu: yea
///
/// - assert:
///     that:
///       - foo == 'boo'
///       - buu[2] == 3
///       - zoo.suu == 'yea'
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::jinja::render;
use crate::modules::{Module, ModuleResult};

use minijinja::Value;

use minijinja::context;
#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
use serde_yaml::Value as YamlValue;

#[derive(Debug)]
pub struct SetVars;

impl Module for SetVars {
    fn get_name(&self) -> &str {
        "set_vars"
    }

    fn exec(
        &self,
        params: YamlValue,
        vars: Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        let mut new_vars = vars.clone();

        match params {
            YamlValue::Mapping(map) => {
                map.iter()
                    .map(|hash_map| {
                        let key = hash_map.0.as_str().ok_or_else(|| {
                            Error::new(
                                ErrorKind::InvalidData,
                                format!("{:?} is not a valid string", &hash_map.0),
                            )
                        })?;
                        let element = json!({key: render(hash_map.1.clone(), &vars)?});
                        new_vars = context! {..Value::from_serialize(element), ..new_vars.clone()};
                        Ok(())
                    })
                    .collect::<Result<Vec<_>>>()?;
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("{:?} must be a dict", &params),
                ));
            }
        }

        Ok((
            ModuleResult {
                changed: false,
                output: None,
                extra: None,
            },
            new_vars,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<RootSchema> {
        None
    }
}
