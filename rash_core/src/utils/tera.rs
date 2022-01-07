use crate::error;
use crate::error::Result;
use crate::vars::Vars;

use std::collections::HashMap;
use std::error::Error as StdError;

use serde_json::value::Value;
use tera::Tera;

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
pub fn render_string(s: &str, vars: &Vars) -> Result<String> {
    let mut tera = TERA.clone();
    trace!("rendering {:?}", &s);
    tera.render_str(s, vars).map_err(|e| match e.source() {
        Some(source_e) => match source_e.source() {
            Some(source_source_e) => match source_source_e.source() {
                Some(source_source_source_e) => {
                    if source_source_source_e.to_string() == "Not defined" {
                        error::Error::new(error::ErrorKind::OmitParam, "Param is omitted")
                    } else {
                        error::Error::new(error::ErrorKind::InvalidData, e)
                    }
                }
                _ => error::Error::new(error::ErrorKind::InvalidData, e),
            },
            _ => error::Error::new(error::ErrorKind::InvalidData, e),
        },
        _ => error::Error::new(error::ErrorKind::InvalidData, e),
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
        &format!("{{% if {} | safe %}}true{{% else %}}false{{% endif %}}", s),
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
        dbg!(&e);
        assert_eq!(e.kind(), error::ErrorKind::OmitParam)
    }
}
