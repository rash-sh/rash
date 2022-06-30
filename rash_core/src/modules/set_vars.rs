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
use crate::modules::ModuleResult;
use crate::utils::get_serde_yaml;
use crate::vars::Vars;

use yaml_rust::Yaml;

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
            new_vars.insert(
                hash_map.0.as_str().ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("{:?} is not a valid string", &hash_map.0),
                    )
                })?,
                &get_serde_yaml(hash_map.1)?,
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
