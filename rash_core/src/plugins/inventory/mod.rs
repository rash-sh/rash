pub mod env;
use crate::constants::ENV_VAR_PREFIX;

use std::any::Any;
use std::collections::HashMap;

pub type Facts = HashMap<String, String>;

#[derive(Debug)]
pub struct Inventory {
    load_fn: fn() -> Facts,
}

impl Inventory {
    pub fn new(load_fn: fn() -> Facts) -> Self {
        Inventory { load_fn: load_fn }
    }

    pub fn load(&self) -> Facts {
        debug!("loading inventory");
        (self.load_fn)()
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Inventory {
            load_fn: || {
                [("foo", "boo"), ("xuu", "zoo")]
                    .iter()
                    .cloned()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<HashMap<String, String>>()
            },
        }
    }
}

lazy_static! {
    pub static ref INVENTORIES: HashMap<&'static str, Inventory> = {
        let mut m = HashMap::new();
        m.insert("env", Inventory { load_fn: env::load });
        m
    };
}
