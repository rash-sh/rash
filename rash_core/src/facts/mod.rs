pub mod env;

use crate::error::Result;

use std::collections::HashMap;
use tera::Context;

pub type Facts = Context;

#[cfg(test)]
pub fn test_example() -> Facts {
    Context::from_serialize(
        [("foo", "boo"), ("xuu", "zoo")]
            .iter()
            .cloned()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<String, String>>(),
    )
    .unwrap()
}

lazy_static! {
    pub static ref FACTS_SOURCES: HashMap<&'static str, fn() -> Result<Facts>> = {
        let mut m = HashMap::new();
        m.insert("env", env::load as fn() -> Result<Facts>);
        m
    };
}
