mod find;
mod password;
#[cfg(feature = "passwordstore")]
mod passwordstore;

mod utils;

use rash_derive::generate_lookup_functions;

generate_lookup_functions!((find, false), (password, false), (passwordstore, true),);
