use crate::error::{Error, ErrorKind, Result};

use std::path::Path;

use libc::{getgid, getuid};
use serde::Serialize;

// ANCHOR: builtins
#[derive(Serialize)]
pub struct Builtins {
    /// Args passed from command line execution.
    args: Vec<String>,
    /// Script directory absolute path.
    dir: String,
    /// Script absolute path.
    path: String,
    user: UserInfo,
}

#[derive(Serialize)]
struct UserInfo {
    uid: u32,
    gid: u32,
}
// ANCHOR_END: builtins

/// # Examples
///
// ANCHOR: examples
/// ```yaml
/// - assert:
///     that:
///       - 'rash.args | length == 0'
///       - 'rash.dir == "/"'
///       - 'rash.path == "/builtins_example.rh"'
///       - 'rash.user.uid == 1000'
///       - 'rash.user.gid == 1000'
/// ```
// ANCHOR_END: examples

impl Builtins {
    pub fn new(args: Vec<&str>, path: &Path) -> Result<Self> {
        let dir = path
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Script parent dir not found"))?
            .to_str()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Script parent dir cannot be parsed to String",
                )
            })?
            .to_string();

        let uid: u32;
        let gid: u32;

        unsafe {
            uid = getuid();
            gid = getgid();
        }

        Ok(Builtins {
            args: args.into_iter().map(String::from).collect(),
            dir: if dir.is_empty() { ".".to_string() } else { dir },
            path: path
                .to_str()
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "Script path cannot be parsed to String",
                    )
                })?
                .to_string(),
            user: UserInfo { uid, gid },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_new() {
        let builtins = Builtins::new(vec![], Path::new("/example.rh")).unwrap();
        assert_eq!(builtins.args.len(), 0);
        assert_eq!(builtins.path, "/example.rh".to_string());
        assert_eq!(builtins.dir, "/".to_string());
    }

    #[test]
    fn test_builtin_same_dir() {
        let builtins = Builtins::new(vec![], Path::new("example.rh")).unwrap();
        assert_eq!(builtins.dir, ".".to_string());
    }
}
