use crate::constants::ENV_VAR_PREFIX;
use crate::error::{Error, ErrorKind, Result};
use crate::facts::Facts;

use std::collections::HashMap;
use std::env;

use tera::Context;

pub fn load<'a>() -> Result<Facts> {
    Ok(Context::from_serialize(
        env::vars()
            .filter(|(envar, _)| envar.starts_with(ENV_VAR_PREFIX))
            .map(|(key, value)| match key.get(ENV_VAR_PREFIX.len()..) {
                Some(s) => Ok((s.to_string().to_lowercase(), value)),
                None => Err(Error::new(
                    ErrorKind::NotFound,
                    format!("Error found while getting envar {:?}", key),
                )),
            })
            .collect::<Result<HashMap<String, String>>>()?,
    )
    .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::env;

    pub fn run_test_with_envar(envar: (&str, &str), test_fn: fn()) -> () {
        env::set_var(&envar.0, &envar.1);
        test_fn();
        env::remove_var(&envar.0);
    }

    #[test]
    fn test_inventory_from_envars() {
        run_test_with_envar((&format!("{}KEY", ENV_VAR_PREFIX), "VALUE"), || {
            let json = load().unwrap().into_json();
            let result = json.get("key").unwrap();

            assert_eq!(result, "VALUE");
        });
    }

    #[test]
    fn test_inventory_from_envars_none() {
        run_test_with_envar(("KEY_NOT_FOUND", "VALUE"), || {
            let facts = load().unwrap();
            assert!(facts.into_json().get("key_not_found").is_none());
        });
    }
}
