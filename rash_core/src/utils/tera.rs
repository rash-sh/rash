use crate::error;
use crate::error::{Error, ErrorKind, Result};
use crate::vars::Vars;

use std::collections::HashMap;
use std::error::Error as StdError;

use serde_yaml::value::Value;
use tera::Tera;

fn omit(_: &HashMap<String, serde_json::value::Value>) -> tera::Result<serde_json::value::Value> {
    Err(tera::Error::call_filter(
        "omit",
        tera::Error::msg("Not defined"),
    ))
}

fn init_tera() -> Tera {
    let mut tera = Tera::default();
    tera.register_function("omit", omit);
    tera
}

lazy_static! {
    static ref TERA: Tera = init_tera();
}

#[inline(always)]
pub fn render(value: Value, vars: &Vars) -> Result<Value> {
    match value {
        Value::String(s) => Ok(Value::String(render_string(&s, vars)?)),
        Value::Number(_) => Ok(value),
        Value::Bool(_) => Ok(value),
        Value::Sequence(v) => Ok(Value::Sequence(
            v.iter()
                .map(|x| render(x.clone(), vars))
                .collect::<Result<Vec<_>>>()?,
        )),
        Value::Mapping(x) => Ok(Value::Mapping(
            x.iter()
                .map(|t| render((*t.1).clone(), vars).map(|value| ((*t.0).clone(), value)))
                .collect::<Result<_>>()?,
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{value:?} is not a valid render value"),
        )),
    }
}

#[inline(always)]
pub fn render_string(s: &str, vars: &Vars) -> Result<String> {
    let mut tera = TERA.clone();
    trace!("rendering {:?}", &s);
    tera.render_str(s, vars).map_err(|e| {
        let f = |e: &dyn StdError| -> Option<bool> {
            Some(e.source()?.source()?.source()?.to_string() == "Not defined")
        };
        match f(&e) {
            Some(true) => error::Error::new(error::ErrorKind::OmitParam, "Param is omitted"),
            _ => error::Error::new(error::ErrorKind::InvalidData, e),
        }
    })
}

#[inline(always)]
pub fn render_as_json(s: &str, vars: &Vars) -> Result<String> {
    render_string(&s.replace("}}", "| json_encode() | safe }}"), vars)
}

#[inline(always)]
pub fn is_render_string(s: &str, vars: &Vars) -> Result<bool> {
    match render_string(
        // tera v2 will fix this allowing ({})
        &format!("{{% if {s} | safe %}}true{{% else %}}false{{% endif %}}"),
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
    fn test_render() {
        let r_yaml = render(Value::from(1), &Vars::new()).unwrap();
        assert_eq!(r_yaml, Value::from(1));

        let r_yaml = render(Value::from("yea"), &Vars::new()).unwrap();
        assert_eq!(r_yaml, Value::from("yea"));
    }

    #[test]
    fn test_is_render_string() {
        let r_true = is_render_string("true", &Vars::new()).unwrap();
        assert!(r_true);
        let r_false = is_render_string("false", &Vars::new()).unwrap();
        assert!(!r_false);
    }

    #[test]
    fn test_render_string_omit() {
        let string = "{{ package_filters | default(value=omit()) }}";
        let e = render_string(string, &Vars::new()).unwrap_err();
        assert_eq!(e.kind(), error::ErrorKind::OmitParam)
    }
}
