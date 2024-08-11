/// ANCHOR: lookup
/// # passwordstore
///
/// Lookup passwords from the passwordstore.org pass utility.
///
/// ANCHOR_END: lookup
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - debug:
///     msg: "{{ passwordstore('foo/boo') }}"
/// ```
/// ANCHOR_END: examples
use std::env;
use std::result::Result as StdResult;

use minijinja::{Error as MinijinjaError, ErrorKind as MinijinjaErrorKind, Value};
use prs_lib::crypto::{self, Config, IsContext, Proto};
use prs_lib::{Store, STORE_DEFAULT_ROOT};

pub fn function(path: String) -> StdResult<Value, MinijinjaError> {
    let store_path = env::var("PASSWORD_STORE_DIR").unwrap_or(STORE_DEFAULT_ROOT.to_string());
    let store = Store::open(store_path).map_err(|_| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            "Password store failed to open",
        )
    })?;

    let secret = store.find_at(&path).ok_or_else(|| {
        MinijinjaError::new(MinijinjaErrorKind::InvalidOperation, "Secret not found")
    })?;

    let config = Config::from(Proto::Gpg);
    let plaintext = crypto::context(&config)
        .map_err(|e| MinijinjaError::new(MinijinjaErrorKind::InvalidOperation, e.to_string()))?
        .decrypt_file(&secret.path)
        .map_err(|e| MinijinjaError::new(MinijinjaErrorKind::InvalidOperation, e.to_string()))?;
    let first_line = plaintext
        .first_line()
        .map_err(|e| MinijinjaError::new(MinijinjaErrorKind::InvalidOperation, e.to_string()))?;

    let password = first_line
        .unsecure_to_str()
        .map_err(|e| MinijinjaError::new(MinijinjaErrorKind::InvalidOperation, e.to_string()))?;
    Ok(Value::from(password))
}
