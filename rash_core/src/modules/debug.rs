/// ANCHOR: module
/// # debug
///
/// This module prints statements during execution and can be useful for debugging variables or
/// expressions. Useful for debugging together with the `when` directive.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - debug:
///     msg: "{{ rash.user.uid }}"
///
/// - debug:
///     var: rash.user.gid
///
/// - debug:
///     var: __tera_context
/// ```
/// ANCHOR_END: examples
use crate::error::Result;
use crate::modules::{parse_params, Module, ModuleResult};
use crate::utils::jinja2::render_string;
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
    #[serde(flatten)]
    pub required: Required,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Required {
    /// The customized message that is printed. If omitted, prints a generic message.
    Msg(String),
    /// A variable name to debug.
    /// Mutually exclusive with the msg option.
    Var(String),
}

fn debug(params: Params, vars: &Vars) -> Result<ModuleResult> {
    let output = match params.required {
        Required::Msg(s) => s,
        Required::Var(var) => render_string(&format!("{{{{ {var} }}}}"), vars)?,
    };

    Ok(ModuleResult {
        changed: false,
        output: Some(output),
        extra: None,
    })
}
#[derive(Debug)]
pub struct Debug;

impl Module for Debug {
    fn get_name(&self) -> &str {
        "debug"
    }

    fn exec(
        &self,
        optional_params: Value,
        vars: Vars,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Vars)> {
        Ok((debug(parse_params(optional_params)?, &vars)?, vars))
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
            msg: foo boo
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                required: Required::Msg("foo boo".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: Value = serde_yaml::from_str(
            r#"
            var: rash.args
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                required: Required::Var("rash.args".to_owned()),
            }
        );
    }

    #[test]
    fn test_debug_msg() {
        let vars = Vars::UNDEFINED;
        let output = debug(
            Params {
                required: Required::Msg("foo boo".to_owned()),
            },
            &vars,
        )
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: Some("foo boo".to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_debug_vars() {
        let vars = Vars::from_serialize(json!({"yea": "foo"}));
        let output = debug(
            Params {
                required: Required::Var("yea".to_owned()),
            },
            &vars,
        )
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: Some("foo".to_owned()),
                extra: None,
            }
        );
    }
}
