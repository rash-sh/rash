#[cfg(feature = "docs")]
pub mod lookup;
#[cfg(not(feature = "docs"))]
mod lookup;
pub mod utils;

use crate::error::{Error, ErrorKind, Result};

use std::sync::LazyLock;

use minijinja::{context, Environment, UndefinedBehavior, Value};
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
fn _render_map(map: serde_yaml::Mapping, vars: &Value, force_string: bool) -> Result<YamlValue> {
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
pub fn render_map(map: serde_yaml::Mapping, vars: &Value) -> Result<YamlValue> {
    _render_map(map, vars, false)
}

#[inline(always)]
pub fn render_map_force_string(map: serde_yaml::Mapping, vars: &Value) -> Result<YamlValue> {
    _render_map(map, vars, true)
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
        YamlValue::Mapping(x) => _render_map(x, vars, force_string),
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
    env.add_template("t", s)?;
    let tmpl = env.get_template("t")?;
    tmpl.render(vars).map(skip_omit)?
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
        let r_yaml =
            render_map(yaml.as_mapping().unwrap().to_owned(), &context! {boo => 1}).unwrap();
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
        let r_yaml =
            render_map(yaml.as_mapping().unwrap().to_owned(), &context! {boo => 2}).unwrap();
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
}
