mod file;
mod find;
mod password;
#[cfg(feature = "passwordstore")]
mod passwordstore;
mod pipe;

mod utils;

use rash_derive::generate_lookup_functions;

generate_lookup_functions!(
    (file, false),
    (find, false),
    (password, false),
    (passwordstore, true),
    (pipe, false)
);
