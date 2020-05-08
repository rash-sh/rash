use crate::constants::ENV_VAR_PREFIX;
use crate::plugins::inventory::Facts;

use std::env;

pub fn load<'a>() -> Facts {
    env::vars()
        .filter(|(envar, _)| envar.starts_with(ENV_VAR_PREFIX))
        .map(|(key, value)| (key.get(ENV_VAR_PREFIX.len()..).unwrap().to_string(), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::plugins::inventory::Inventory;

    use std::env;

    pub fn run_test_with_envar(envar: (&str, &str), test_fn: fn()) -> () {
        env::set_var(&envar.0, &envar.1);
        test_fn();
        env::remove_var(&envar.0);
    }

    #[test]
    fn test_inventory_from_envars() {
        run_test_with_envar((&format!("{}KEY", ENV_VAR_PREFIX), "VALUE"), || {
            let facts = load();
            let result = facts.get("KEY").unwrap();

            assert_eq!(result, "VALUE");
        });
    }

    #[test]
    fn test_inventory_from_envars_none() {
        run_test_with_envar(("KEY_NOT_FOUND", "VALUE"), || {
            let facts = load();
            assert!(facts.get("KEY_NOT_FOUND").is_none());
        });
    }
}
