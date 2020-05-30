use crate::error::{Error, ErrorKind, Result};
use crate::vars::Vars;

use tera::Tera;

pub fn render_string(s: &str, vars: Vars) -> Result<String> {
    let mut tera = Tera::default();
    tera.render_str(s, &vars)
        .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))
}

#[inline(always)]
pub fn is_render_string(s: &str, vars: Vars) -> Result<bool> {
    match render_string(
        &format!("{{% if {} %}}true{{% else %}}false{{% endif %}}", s),
        vars,
    )?
    .as_str()
    {
        "false" => Ok(false),
        _ => Ok(true),
    }
}
