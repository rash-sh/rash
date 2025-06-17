mod error_utils;
#[cfg(feature = "docs")]
pub mod lookup;
#[cfg(not(feature = "docs"))]
mod lookup;

use crate::{
    error::{Error, ErrorKind, Result},
    utils::merge_json,
};
use error_utils::handle_template_error;
use serde::Deserialize;

use std::sync::LazyLock;

use minijinja::{Environment, UndefinedBehavior, Value, context};
use serde_yaml::value::Value as YamlValue;

const OMIT_VALUE: &str = "OMIT_THIS_VARIABLE";

fn init_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.add_global("omit", OMIT_VALUE);
    lookup::add_lookup_functions(&mut env);
    env
}

static MINIJINJA_ENV: LazyLock<Environment<'static>> = LazyLock::new(init_env);

#[inline(always)]
pub fn render_map(map: serde_yaml::Mapping, vars: &Value, force_string: bool) -> Result<YamlValue> {
    let mut rendered_map = serde_yaml::Mapping::new();
    let mut current_vars = vars.clone();

    for (k, v) in map.iter() {
        match _render(v.clone(), &current_vars, force_string) {
            Ok(v) => {
                // safe unwrap: k is always a String
                let value: Value = [(k.as_str().unwrap(), Value::from_serialize(v.clone()))]
                    .into_iter()
                    .collect();
                current_vars = context! {
                    ..current_vars,
                    ..value
                };
                rendered_map.insert(k.clone(), v);
            }
            Err(e) if e.kind() == ErrorKind::OmitParam => (),
            Err(e) => Err(e)?,
        }
    }

    Ok(YamlValue::Mapping(rendered_map))
}

#[inline(always)]
fn _render(value: YamlValue, vars: &Value, force_string: bool) -> Result<YamlValue> {
    match value.clone() {
        YamlValue::String(s) => {
            let rendered = &render_string(&s, vars)?;
            if force_string {
                Ok(YamlValue::String(rendered.to_string()))
            } else {
                Ok(serde_yaml::from_str(rendered)?)
            }
        }
        YamlValue::Number(_) => Ok(value),
        YamlValue::Bool(_) => Ok(value),
        YamlValue::Sequence(v) => Ok(YamlValue::Sequence(
            v.iter()
                .map(|x| _render(x.clone(), vars, force_string))
                .collect::<Result<Vec<_>>>()?,
        )),
        YamlValue::Mapping(x) => render_map(x, vars, force_string),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{value:?} is not a valid render value"),
        )),
    }
}

#[inline(always)]
pub fn render(value: YamlValue, vars: &Value) -> Result<YamlValue> {
    _render(value, vars, false)
}

#[inline(always)]
pub fn render_force_string(value: YamlValue, vars: &Value) -> Result<YamlValue> {
    _render(value, vars, true)
}

fn skip_omit(x: String) -> Result<String> {
    if x == OMIT_VALUE {
        Err(Error::new(ErrorKind::OmitParam, OMIT_VALUE))
    } else {
        Ok(x)
    }
}

#[inline(always)]
pub fn render_string(s: &str, vars: &Value) -> Result<String> {
    let mut env = MINIJINJA_ENV.clone();
    trace!("rendering {:?}", &s);

    env.add_template("t", s)
        .map_err(|e| handle_template_error(e, s, vars))?;
    let tmpl = env
        .get_template("t")
        .map_err(|e| handle_template_error(e, s, vars))?;

    tmpl.render(vars)
        .map(skip_omit)
        .map_err(|e| handle_template_error(e, s, vars))?
}

#[inline(always)]
pub fn is_render_string(s: &str, vars: &Value) -> Result<bool> {
    match render_string(
        &format!("{{% if {s} %}}true{{% else %}}false{{% endif %}}"),
        vars,
    )?
    .as_str()
    {
        "false" => Ok(false),
        _ => Ok(true),
    }
}

pub fn merge_option(a: Value, b: Option<Value>) -> Value {
    if let Some(b) = b { merge(a, b) } else { a }
}

pub fn merge(a: Value, b: Value) -> Value {
    let mut a_json_value: serde_json::Value = serde_json::Value::deserialize(a).unwrap();
    let b_json_value: serde_json::Value = serde_json::Value::deserialize(b).unwrap();
    merge_json(&mut a_json_value, b_json_value);
    Value::from_serialize(a_json_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_map() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            yea: "{{ boo }}"
            "#,
        )
        .unwrap();
        let r_yaml = render_map(
            yaml.as_mapping().unwrap().to_owned(),
            &context! {boo => 1},
            false,
        )
        .unwrap();
        let expected: YamlValue = serde_yaml::from_str(
            r#"
            yea: 1
            "#,
        )
        .unwrap();
        assert_eq!(r_yaml, expected);

        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            yea: "{{ boo }}"
            fuu: "{{ zoo | default(omit) }}"
            "#,
        )
        .unwrap();
        let r_yaml = render_map(
            yaml.as_mapping().unwrap().to_owned(),
            &context! {boo => 2},
            false,
        )
        .unwrap();
        let expected: YamlValue = serde_yaml::from_str(
            r#"
            yea: 2
            "#,
        )
        .unwrap();
        assert_eq!(r_yaml, expected);
    }

    #[test]
    fn test_render() {
        let r_yaml = render(YamlValue::from(1), &context! {}).unwrap();
        assert_eq!(r_yaml, YamlValue::from(1));

        let r_yaml = render(YamlValue::from("yea"), &context! {}).unwrap();
        assert_eq!(r_yaml, YamlValue::from("yea"));
    }

    #[test]
    fn test_render_string() {
        let r_yaml = render_string("{{ yea }}", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, "1");

        let r_yaml = render_string("{{ yea }} ", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, "1 ");

        let r_yaml = render_string(" {{ yea }}", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, " 1");

        let r_yaml = render_string("{{ yea }}\n", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, "1\n");
    }

    #[test]
    fn test_is_render_string() {
        let r_true = is_render_string("true", &context! {}).unwrap();
        assert!(r_true);
        let r_false = is_render_string("false", &context! {}).unwrap();
        assert!(!r_false);
        let r_true = is_render_string("boo == 'test'", &context! {boo => "test"}).unwrap();
        assert!(r_true);
    }

    #[test]
    fn test_render_string_omit() {
        let string = "{{ package_filters | default(omit) }}";

        let e = render_string(string, &context! {}).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::OmitParam);

        let result = render_string(string, &context! {package_filters => "yea"}).unwrap();
        assert_eq!(result, "yea");
    }

    #[test]
    fn test_render_string_undefined_variable_error() {
        // Test single undefined variable
        let error = render_string("{{ undefined_var }}", &context! {}).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::JinjaRenderError);
        assert!(
            error
                .to_string()
                .contains("undefined variable 'undefined_var'")
        );
        assert!(
            error
                .to_string()
                .contains("in template: {{ undefined_var }}")
        );

        // Test undefined variable with defined variables present
        let error = render_string(
            "{{ defined_var }} and {{ undefined_var }}",
            &context! {defined_var => "hello"},
        )
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::JinjaRenderError);
        assert!(
            error
                .to_string()
                .contains("undefined variable 'undefined_var'")
        );
        assert!(error.to_string().contains("in template:"));

        // Test nested undefined variable
        let error = render_string("{{ foo.bar }}", &context! {}).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::JinjaRenderError);
        assert!(error.to_string().contains("undefined variable 'foo.bar'"));

        // Test that variables with default filter don't cause undefined errors
        let result =
            render_string("{{ missing_var | default('fallback') }}", &context! {}).unwrap();
        assert_eq!(result, "fallback");
    }

    #[test]
    fn test_render_string_operation_errors() {
        // Test integer conversion error
        let error = render_string("{{ 'not_a_number' | int }}", &context! {}).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::JinjaRenderError);
        let error_msg = error.to_string();
        assert!(error_msg.contains("invalid operation"));

        // Test float conversion error
        let error = render_string("{{ 'not_a_float' | float }}", &context! {}).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::JinjaRenderError);
        let error_msg = error.to_string();
        assert!(error_msg.contains("invalid operation"));

        // Test that successful conversions still work
        let result = render_string("{{ '42' | int }}", &context! {}).unwrap();
        assert_eq!(result, "42");

        let result = render_string("{{ '3.14' | float }}", &context! {}).unwrap();
        assert_eq!(result, "3.14");
    }
}
