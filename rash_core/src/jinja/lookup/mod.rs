#[cfg(feature = "passwordstore")]
mod passwordstore;

use minijinja::Environment;

pub fn add_lookup_functions(env: &mut Environment<'static>) {
    #[cfg(feature = "passwordstore")]
    env.add_function("passwordstore", passwordstore::passwordstore);
}
