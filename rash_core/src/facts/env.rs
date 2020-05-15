use crate::error::{Error, ErrorKind, Result};
use crate::facts::Facts;

use std::collections::HashMap;
use std::env;

#[macro_use]
use serde::Serialize;
use tera::Context;

#[derive(Serialize)]
struct Env {
    env: HashMap<String, String>,
}

impl From<env::Vars> for Env {
    fn from(envars: env::Vars) -> Self {
        Self {
            env: envars.collect::<HashMap<String, String>>(),
        }
    }
}

pub fn load(envars: Vec<(String, String)>) -> Result<Facts> {
    trace!("{:?}", envars);
    envars.into_iter().for_each(|(k, v)| env::set_var(k, v));
    Ok(Context::from_serialize(&Env::from(env::vars()))
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
        run_test_with_envar(("KEY", "VALUE"), || {
            let json = load(vec![]).unwrap().into_json();
            let result = json.get("env").unwrap().get("KEY").unwrap();

            assert_eq!(result, "VALUE");
        });
    }

    #[test]
    fn test_inventory_from_envars_none() {
        run_test_with_envar(("KEY_NOT_FOUND", "VALUE"), || {
            let facts = load(vec![]).unwrap();
            assert!(facts
                .into_json()
                .get("env")
                .unwrap()
                .get("key_not_found")
                .is_none());
        });
    }
}
