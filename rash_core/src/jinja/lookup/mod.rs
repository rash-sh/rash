#[cfg(feature = "passwordstore")]
mod passwordstore;

use rash_derive::generate_lookup_functions;

generate_lookup_functions!(passwordstore);
