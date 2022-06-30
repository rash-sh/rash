use crate::error;
use crate::error::{Error, ErrorKind, Result};
use crate::vars::Vars;

use std::collections::HashMap;
use std::error::Error as StdError;

use serde_json::value::Value;
use tera::Tera;
use yaml_rust::Yaml;

fn omit(_: &HashMap<String, Value>) -> tera::Result<Value> {
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
pub fn render(yaml: Yaml, vars: &Vars) -> Result<Yaml> {
    match yaml {
        Yaml::String(s) => Ok(Yaml::String(render_string(&s, vars)?)),
        Yaml::Real(x) => Ok(Yaml::Real(x)),
        Yaml::Integer(_) => Ok(yaml),
        Yaml::Boolean(_) => Ok(yaml),
        Yaml::Array(v) => Ok(Yaml::Array(
            v.iter()
                .map(|x| render(x.clone(), vars))
                .collect::<Result<Vec<_>>>()?,
        )),
        Yaml::Hash(x) => Ok(Yaml::Hash(
            x.iter()
                .map(|t| render((*t.1).clone(), vars).map(|yaml| ((*t.0).clone(), yaml)))
                .collect::<Result<_>>()?,
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{yaml:?} is not a valid render value"),
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
        let r_yaml = render(Yaml::Integer(1).clone(), &Vars::new()).unwrap();
        assert_eq!(r_yaml, Yaml::Integer(1));

        let r_yaml = render(Yaml::String("yea".to_string()).clone(), &Vars::new()).unwrap();
        assert_eq!(r_yaml, Yaml::String("yea".to_string()));
    }

    #[test]
    fn test_is_render_string() {
        let r_true = is_render_string("true", &Vars::new()).unwrap();
        assert_eq!(r_true, true);
        let r_false = is_render_string("false", &Vars::new()).unwrap();
        assert_eq!(r_false, false);
    }

    #[test]
    fn test_render_string_omit() {
        let string = "{{ package_filters | default(value=omit()) }}";
        let e = render_string(string, &Vars::new()).unwrap_err();
        assert_eq!(e.kind(), error::ErrorKind::OmitParam)
    }
}
