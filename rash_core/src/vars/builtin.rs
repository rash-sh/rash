use crate::error::{Error, ErrorKind, Result};

use std::path::{Path, PathBuf};

use nix::unistd::{getuid, User};
use serde::Serialize;

#[derive(Serialize)]
struct UserInfo {
    name: String,
    uid: u32,
    gid: u32,
}

#[derive(Serialize)]
pub struct Builtins {
    args: Vec<String>,
    dir: PathBuf,
    path: PathBuf,
    user: UserInfo,
}

impl Builtins {
    pub fn new(args: Vec<&str>, path: &Path) -> Result<Self> {
        let user = User::from_uid(getuid())
            .or_else(|e| Err(Error::new(ErrorKind::Other, e)))?
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "No user with current uid"))?;
        let dir = path
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf();
        Ok(Builtins {
            args: args.into_iter().map(String::from).collect(),
            dir,
            path: path.to_path_buf(),
            user: UserInfo {
                name: user.name,
                uid: user.uid.as_raw(),
                gid: user.gid.as_raw(),
            },
        })
    }
}
