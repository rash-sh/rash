/// ANCHOR: module
/// # assert
///
/// Assert given expressions are true.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - assert:
///     that:
///       - boo is defined
///       - 1 + 1 == 2
///       - env.MY_VAR is defined
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, Module, ModuleResult};
use crate::utils::jinja2::is_render_string;
use crate::vars::Vars;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde_yaml::Value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// A list of string expressions of the same form that can be passed to the
    /// _when_ statement.
    that: Vec<String>,
}

fn verify_conditions(params: Params, vars: &Vars) -> Result<ModuleResult> {
    params.that.iter().try_for_each(|expression| {
        if is_render_string(expression, vars)? {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::Other,
                format!("{} expression is false", &expression),
            ))
        }
    })?;
    Ok(ModuleResult {
        changed: false,
        output: None,
        extra: None,
    })
}

#[derive(Debug)]
pub struct Assert;

impl Module for Assert {
    fn get_name(&self) -> &str {
        "assert"
    }

    fn exec(
        &self,
        optional_params: Value,
        vars: Vars,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Vars)> {
        Ok((
            verify_conditions(parse_params(optional_params)?, &vars)?,
            vars,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<RootSchema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: Value = serde_yaml::from_str(
            r#"
            that:
              - 1 == 1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                that: vec!["1 == 1".to_owned()],
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: Value = serde_yaml::from_str(
            r#"
            that:
              - 1 == 1
            yea: boo
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_verify_conditions() {
        let _ = verify_conditions(
            Params {
                that: vec!["1 == 1".to_owned()],
            },
            &Vars::from_serialize(json!({})),
        )
        .unwrap();
    }

    #[test]
    fn test_verify_conditions_fail() {
        let _ = verify_conditions(
            Params {
                that: vec!["1 != 1".to_owned()],
            },
            &Vars::from_serialize(json!({})),
        )
        .unwrap_err();
    }
}
