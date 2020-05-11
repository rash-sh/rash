pub mod env;

use crate::error::Result;

use std::collections::HashMap;

pub type Facts = HashMap<String, String>;

#[cfg(test)]
pub fn test_example() -> Facts {
    [("foo", "boo"), ("xuu", "zoo")]
        .iter()
        .cloned()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<Facts>()
}

lazy_static! {
    pub static ref FACTS_SOURCES: HashMap<&'static str, fn() -> Result<Facts>> = {
        let mut m = HashMap::new();
        m.insert("env", env::load as fn() -> Result<Facts>);
        m
    };
}
