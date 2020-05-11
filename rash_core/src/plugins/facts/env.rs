use crate::constants::ENV_VAR_PREFIX;
use crate::error::{Error, ErrorKind, Result};
use crate::plugins::facts::Facts;

use std::env;

pub fn load<'a>() -> Result<Facts> {
    env::vars()
        .filter(|(envar, _)| envar.starts_with(ENV_VAR_PREFIX))
        .map(|(key, value)| match key.get(ENV_VAR_PREFIX.len()..) {
            Some(s) => Ok((s.to_string(), value)),
            None => Err(Error::new(
                ErrorKind::NotFound,
                format!("Error found while getting envar {:?}", key),
            )),
        })
        .collect::<Result<Facts>>()
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
            let facts = load().unwrap();
            let result = facts.get("KEY").unwrap();

            assert_eq!(result, "VALUE");
        });
    }

    #[test]
    fn test_inventory_from_envars_none() {
        run_test_with_envar(("KEY_NOT_FOUND", "VALUE"), || {
            let facts = load().unwrap();
            assert!(facts.get("KEY_NOT_FOUND").is_none());
        });
    }
}
