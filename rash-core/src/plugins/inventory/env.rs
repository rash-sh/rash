use crate::constants::ENV_VAR_PREFIX;

use std::collections::HashMap;

#[derive(Debug)]
pub struct Inventory {
    facts: HashMap<String, String>,
}

impl Inventory {
    pub fn new<'a, I>(envars: I) -> Self
    where
        I: Iterator<Item = (String, String)>,
    {
        Inventory {
            facts: envars
                .filter(|(envar, _)| envar.starts_with(ENV_VAR_PREFIX))
                .map(|(key, value)| (key.get(ENV_VAR_PREFIX.len()..).unwrap().to_string(), value))
                .collect(),
        }
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Inventory {
            facts: [("foo", "boo"), ("xuu", "zoo")]
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<String, String>>(),
        }
    }
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
            let inventory = Inventory::new(env::vars());
            let result = inventory.facts.get("KEY").unwrap();

            assert_eq!(result, "VALUE");
        });
    }

    #[test]
    fn test_inventory_from_envars_none() {
        run_test_with_envar(("KEY", "VALUE"), || {
            let inventory = Inventory::new(env::vars());
            assert!(inventory.facts.get("KEY").is_none());
        });
    }

    #[test]
    fn test_inventory_from_iterator() {
        let iterator: Vec<(String, String)> = vec![(
            format!("{}KEY", ENV_VAR_PREFIX).to_string(),
            "VALUE".to_string(),
        )];

        let inventory = Inventory::new(iterator.iter().cloned());
        let result = inventory.facts.get("KEY").unwrap();

        assert_eq!(result, "VALUE");
    }
}
