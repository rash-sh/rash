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

#[derive(Debug)]
pub enum EnvInput {
    EnvVars(env::Vars),
    VecVars(Vec<(String, String)>),
}

impl From<EnvInput> for Env {
    fn from(envars: EnvInput) -> Self {
        match envars {
            EnvInput::EnvVars(envars) => Self {
                env: envars.collect::<HashMap<String, String>>(),
            },
            EnvInput::VecVars(envars_vec) => Self {
                env: envars_vec.into_iter().collect::<HashMap<String, String>>(),
            },
        }
    }
}

pub fn load_generic(envars: EnvInput) -> Result<Facts> {
    trace!("{:?}", envars);
    Ok(Context::from_serialize(&Env::from(match envars {
        EnvInput::EnvVars(envars) => EnvInput::EnvVars(envars),
        EnvInput::VecVars(envars) => EnvInput::VecVars(envars),
    }))
    .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?)
}

pub fn load<'a>() -> Result<Facts> {
    load_generic(EnvInput::EnvVars(env::vars()))
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
            let json = load().unwrap().into_json();
            let result = json.get("env").unwrap().get("KEY").unwrap();

            assert_eq!(result, "VALUE");
        });
    }

    #[test]
    fn test_inventory_from_envars_none() {
        run_test_with_envar(("KEY_NOT_FOUND", "VALUE"), || {
            let facts = load().unwrap();
            assert!(facts
                .into_json()
                .get("env")
                .unwrap()
                .get("key_not_found")
                .is_none());
        });
    }
}
