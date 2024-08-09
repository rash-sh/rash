use minijinja::Value;

use std::collections::HashMap;
use std::env;

use serde::Serialize;

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

/// Create [`Vars`] from environment variables plus input vector overwriting them.
///
/// [`Vars`]: ../type.Vars.html
///
/// # Example
///
/// ```
/// use rash_core::vars::env::load;
///
/// use std::env;
///
/// let vars = load(vec![("foo".to_owned(), "boo".to_owned())]);
/// ```
pub fn load(envars: Vec<(String, String)>) -> Value {
    trace!("{:?}", envars);
    envars.into_iter().for_each(|(k, v)| env::set_var(k, v));
    Value::from_serialize(Env::from(env::vars()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::env;

    pub fn run_test_with_envar(envar: (&str, &str), test_fn: fn()) {
        env::set_var(envar.0, envar.1);
        test_fn();
        env::remove_var(envar.0);
    }

    #[test]
    fn test_inventory_from_envars() {
        run_test_with_envar(("KEY", "VALUE"), || {
            let vars = load(vec![]);
            let result = vars.get_attr("env").unwrap().get_attr("KEY").unwrap();

            assert_eq!(result.to_string(), "VALUE");
        });
    }

    #[test]
    fn test_inventory_from_envars_none() {
        run_test_with_envar(("KEY_NOT_FOUND", "VALUE"), || {
            let vars = load(vec![]);
            assert_eq!(
                vars.get_attr("env")
                    .unwrap()
                    .get_attr("key_not_found")
                    .unwrap(),
                Value::UNDEFINED
            );
        });
    }
}
