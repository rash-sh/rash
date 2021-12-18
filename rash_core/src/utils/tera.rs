use crate::error::{Error, ErrorKind, Result};
use crate::vars::Vars;

use tera::Tera;

#[inline(always)]
pub fn render_string(s: &str, vars: &Vars) -> Result<String> {
    let mut tera = Tera::default();
    trace!("rendering {:?}", &s);
    tera.render_str(s, vars)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

#[inline(always)]
pub fn render_as_json(s: &str, vars: &Vars) -> Result<String> {
    render_string(&s.replace("}}", "| json_encode() | safe }}"), vars)
}

#[inline(always)]
pub fn is_render_string(s: &str, vars: &Vars) -> Result<bool> {
    match render_string(
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
}
