/// ANCHOR: module
/// # assert
///
/// Assert given expressions are true.
///
/// ## Parameters
///
/// ```yaml
/// that:
///   type: list
///   required: true
///   description: |
///     A list of string expressions of the same form that can be passed to the
///     'when' statement.
/// ```
///
/// ## Example
///
/// ```yaml
/// - assert:
///     that:
///       - boo is defined
///       - 1 + 1 == 2
///       - env.MY_VAR is defined
/// ```
/// ANCHOR_END: module
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{get_param_list, ModuleResult};
use crate::utils::tera::is_render_string;
use crate::vars::Vars;

use yaml_rust::Yaml;

#[derive(Debug, PartialEq)]
pub struct Params {
    that: Vec<String>,
}

fn parse_params(yaml: Yaml) -> Result<Params> {
    trace!("parse params: {:?}", yaml);

    Ok(Params {
        that: get_param_list(&yaml, "that")?,
    })
}

fn verify_conditions(params: Params, vars: Vars) -> Result<ModuleResult> {
    let _ = params
        .that
        .iter()
        .map(|expression| {
            if is_render_string(expression, vars.clone())? {
                Ok(true)
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    format!("{} expression is false", &expression),
                ))
            }
        })
        .collect::<Result<Vec<bool>>>()?;
    Ok(ModuleResult {
        changed: false,
        output: None,
        extra: None,
    })
}

pub fn exec(optional_params: Yaml, vars: Vars) -> Result<(ModuleResult, Vars)> {
    Ok((
        verify_conditions(parse_params(optional_params)?, vars.clone())?,
        vars,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use yaml_rust::YamlLoader;

    #[test]
    fn test_parse_params() {
        let yaml = YamlLoader::load_from_str(
            r#"
        that:
          - 1 == 1
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                that: vec!["1 == 1".to_string()],
            }
        );
    }

    #[test]
    fn test_verify_conditions() {
        let _ = verify_conditions(
            Params {
                that: vec!["1 == 1".to_string()],
            },
            Vars::new(),
        )
        .unwrap();
    }

    #[test]
    fn test_verify_conditions_fail() {
        let _ = verify_conditions(
            Params {
                that: vec!["1 != 1".to_string()],
            },
            Vars::new(),
        )
        .unwrap_err();
    }
}
