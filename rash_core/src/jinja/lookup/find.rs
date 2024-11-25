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
///     msg: "{{ find({'paths': '/'}) }}"
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
use crate::modules::find::{find, Params};

use std::ops::Deref;
use std::result::Result as StdResult;

use minijinja::value::ViaDeserialize;
use minijinja::{Error as MinijinjaError, Value};

pub fn function(config: ViaDeserialize<Params>) -> StdResult<Value, MinijinjaError> {
    let params = config.deref();
    find(params.clone())
        .map_err(to_minijinja_error)
        .map(|x| Value::from_serialize(x.get_extra()))
        .map_err(to_minijinja_error)
}
