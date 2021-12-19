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
///
/// - assert
///     that:
///       - foo == 'boo'
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::ModuleResult;
use crate::vars::Vars;

use serde_yaml::Value;
use yaml_rust::{Yaml, YamlEmitter};

pub fn exec(params: Yaml, vars: Vars, _check_mode: bool) -> Result<(ModuleResult, Vars)> {
    let mut new_vars = vars;

    params
        .as_hash()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("{:?} must be a dict", &params),
            )
        })
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
        .iter()
        .map(|hash_map| {
            let mut yaml_str = String::new();
            let mut emitter = YamlEmitter::new(&mut yaml_str);
            emitter
                .dump(hash_map.1)
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
            let yaml: Value = serde_yaml::from_str(&yaml_str)
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

            new_vars.insert(
                hash_map.0.as_str().ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("{:?} is not a valid string", &hash_map.0),
                    )
                })?,
                &yaml,
            );
            Ok(())
        })
        .collect::<Result<Vec<_>>>()?;

    Ok((
        ModuleResult {
            changed: false,
            output: None,
            extra: None,
        },
        new_vars,
    ))
}
