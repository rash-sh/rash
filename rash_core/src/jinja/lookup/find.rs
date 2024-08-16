/// ANCHOR: lookup
/// # find
///
/// Use [find module](./module_find.html) as a lookup. Returns the extra field of the module result.
///
/// ANCHOR_END: lookup
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - debug:
///     msg: "{{ find(paths='/') }}"
///
/// - name: Copy all files in /tmp to /tmp2
///   vars:
///     tmp_query:
///       paths: "/tmp"
///       hidden: true
///       recurse: false
///   loop: "{{ find(tmp_query) }}"
///   copy:
///     src: "{{ item }}""
///     dest: "/tmp2/{{ item | basename }}"
///
/// ```
/// ANCHOR_END: examples
use crate::jinja::lookup::utils::to_minijinja_error;
use crate::modules::find::find;
use crate::modules::parse_params;

use std::result::Result as StdResult;

use minijinja::{Error as MinijinjaError, Value};

pub fn function(config: Value) -> StdResult<Value, MinijinjaError> {
    parse_params(serde_yaml::to_value(config).map_err(to_minijinja_error)?)
        .map_err(to_minijinja_error)
        .and_then(|params| {
            Ok(find(params)
                .map_err(to_minijinja_error)
                .map(|x| serde_yaml::to_value(x.get_extra()))
                .map_err(to_minijinja_error)?
                .map(Value::from_serialize))
        })
        .map_err(to_minijinja_error)?
        .map_err(to_minijinja_error)
}
