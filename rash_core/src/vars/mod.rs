pub mod builtin;
pub mod env;

use tera::Context;

/// Variables stored and accessible during execution, based on [`tera::Context`]
///
/// [`tera::Context`]: ../../tera/struct.Context.html
pub type Vars = Context;

#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
pub fn from_iter<'a, I>(iterable: I) -> Vars
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    Context::from_serialize(
        iterable
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect::<HashMap<String, String>>(),
    )
    .unwrap()
}
