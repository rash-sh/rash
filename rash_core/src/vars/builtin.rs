use crate::error::{Error, ErrorKind, Result};

use std::fs::canonicalize;
use std::path::Path;

use nix::unistd::{getgid, getuid};
use serde::{Deserialize, Serialize};

// ANCHOR: builtins
#[derive(Serialize, Deserialize)]
pub struct Builtins {
    /// Args passed from command line execution.
    args: Vec<String>,
    /// Script directory absolute path.
    dir: String,
    /// Script absolute path.
    path: String,
    user: UserInfo,
}

#[derive(Serialize, Deserialize)]
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
        Ok(Builtins {
            args: args.into_iter().map(String::from).collect(),
            dir: Builtins::get_dir(path)?,
            path: path
                .to_str()
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "Script path cannot be parsed to String",
                    )
                })?
                .to_owned(),
            user: UserInfo {
                uid: u32::from(getuid()),
                gid: u32::from(getgid()),
            },
        })
    }

    fn get_dir(path: &Path) -> Result<String> {
        let parent_path_original = path
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Script parent dir not found"))?;

        let parent_path = if parent_path_original.to_str() == Some("") {
            Path::new(".")
        } else {
            parent_path_original
        };

        trace!("parent_path: {parent_path:?}");

        let dir = canonicalize(parent_path)?
            .to_str()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Script parent dir cannot be represented as UTF-8",
                )
            })?
            .to_owned();

        trace!("dir: {dir:?}");
        Ok(if dir.is_empty() { ".".to_owned() } else { dir })
    }

    pub fn update(&self, path: &Path) -> Result<Self> {
        Ok(Builtins {
            args: self.args.clone(),
            dir: Builtins::get_dir(path)?,
            path: path
                .to_str()
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "Script path cannot be parsed to String",
                    )
                })?
                .to_owned(),
            user: UserInfo {
                uid: u32::from(getuid()),
                gid: u32::from(getgid()),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn test_builtin_new() {
        let builtins = Builtins::new(vec![], Path::new("/example.rh")).unwrap();
        assert_eq!(builtins.args.len(), 0);
        assert_eq!(builtins.path, "/example.rh".to_owned());
        assert_eq!(builtins.dir, "/".to_owned());
    }

    #[test]
    fn test_builtin_same_dir() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        let file_path = dir_path.join("example.rh");
        let builtins = Builtins::new(vec![], file_path.as_ref()).unwrap();
        assert_eq!(
            builtins.dir,
            canonicalize(dir_path).unwrap().to_str().unwrap().to_owned()
        );
    }
}
