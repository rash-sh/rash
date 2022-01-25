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
/// ```
/// ANCHOR_END: examples
use crate::error::Result;
use crate::modules::{parse_params, ModuleResult};
use crate::utils::tera::render_string;
use crate::vars::Vars;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use yaml_rust::Yaml;
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
    /// Be aware that this option already runs in Tera context and has an implicit `{{ }}`
    /// wrapping, so you should not be using Tera delimiters unless you are looking for double
    /// interpolation.
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

pub fn exec(optional_params: Yaml, vars: Vars, _check_mode: bool) -> Result<(ModuleResult, Vars)> {
    Ok((debug(parse_params(optional_params)?, &vars)?, vars))
}

#[cfg(test)]
mod tests {
    use super::*;

    use yaml_rust::YamlLoader;

    #[test]
    fn test_parse_params() {
        let yaml = YamlLoader::load_from_str(
            r#"
msg: foo boo
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                required: Required::Msg("foo boo".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_params_default() {
        let yaml = YamlLoader::load_from_str(
            r#"
var: rash.args
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                required: Required::Var("rash.args".to_string()),
            }
        );
    }

    #[test]
    fn test_debug_msg() {
        let vars = Vars::new();
        let output = debug(
            Params {
                required: Required::Msg("foo boo".to_string()),
            },
            &vars,
        )
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: Some("foo boo".to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_debug_vars() {
        let vars = Vars::from_value(json!({"yea": "foo"})).unwrap();
        let output = debug(
            Params {
                required: Required::Var("yea".to_string()),
            },
            &vars,
        )
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: Some("foo".to_string()),
                extra: None,
            }
        );
    }
}
