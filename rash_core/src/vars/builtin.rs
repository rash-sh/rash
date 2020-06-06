use crate::error::Result;

use std::path::{Path, PathBuf};

use libc::{getgid, getuid};
use serde::Serialize;

// ANCHOR: builtins
#[derive(Serialize)]
pub struct Builtins {
    /// Args passed from command line execution.
    args: Vec<String>,
    /// Script directory absolute path.
    dir: PathBuf,
    /// Script absolute path.
    path: PathBuf,
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
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf();

        let uid: u32;
        let gid: u32;

        unsafe {
            uid = getuid();
            gid = getgid();
        }

        Ok(Builtins {
            args: args.into_iter().map(String::from).collect(),
            dir,
            path: path.to_path_buf(),
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
        assert_eq!(builtins.path.as_os_str(), "/example.rh");
        assert_eq!(builtins.dir.as_os_str(), "/");
    }
}
