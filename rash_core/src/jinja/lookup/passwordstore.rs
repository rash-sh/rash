/// ANCHOR: lookup
/// # passwordstore
///
/// Lookup passwords from the passwordstore.org pass utility.
///
/// ## Parameters
///
/// | Parameter | Required | Type    | Values | Description                                                                                                              |
/// | --------- | -------- | ------- | ------ | ------------------------------------------------------------------------------------------------------------------------ |
/// | returnall |          | boolean |        | Return all the content of the password, not only the first line. **[default: `false`]**                                  |
/// | subkey    |          | string  |        | Return a specific subkey of the password. When set to password, always returns the first line. **[default: `password`]** |
///
/// ANCHOR_END: lookup
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Return the first line of the secret
///   debug:
///     msg: "{{ passwordstore('foo/boo') }}"
///
/// - name: Return all the content
///   debug:
///     msg: "{{ passwordstore('foo/boo', returnall=true) }}"
///
/// - name: Return just the username
///   debug:
///     msg: "{{ passwordstore('foo/boo', subkey='username') }}"
/// ```
/// ANCHOR_END: examples
use crate::jinja::lookup::utils::to_minijinja_error;

use std::env;
use std::result::Result as StdResult;

use minijinja::{Error as MinijinjaError, ErrorKind as MinijinjaErrorKind, Value, value::Kwargs};
use prs_lib::crypto::{self, Config, IsContext, Proto};
use prs_lib::{STORE_DEFAULT_ROOT, Store};

pub fn function(path: String, options: Kwargs) -> StdResult<Value, MinijinjaError> {
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
    let mut plaintext = crypto::context(&config)
        .map_err(to_minijinja_error)?
        .decrypt_file(&secret.path)
        .map_err(to_minijinja_error)?;

    if let Some(key) = options.get::<Option<String>>("subkey")? {
        if key == "password" {
            plaintext = plaintext.first_line().map_err(to_minijinja_error)?;
        } else {
            plaintext = plaintext.property(&key).map_err(to_minijinja_error)?;
        }
    };

    if Some(true) != options.get("returnall")? {
        plaintext = plaintext.first_line().map_err(to_minijinja_error)?;
    };

    let password = plaintext.unsecure_to_str().map_err(to_minijinja_error)?;

    options.assert_all_used()?;

    Ok(Value::from(password))
}
