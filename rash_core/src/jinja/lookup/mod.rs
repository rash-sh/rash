mod find;
#[cfg(feature = "passwordstore")]
mod passwordstore;

mod utils;

use rash_derive::generate_lookup_functions;

generate_lookup_functions!((find, false), (passwordstore, true),);
