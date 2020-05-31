use crate::error::Result;

use std::path::{Path, PathBuf};

use libc::{getgid, getuid};
use serde::Serialize;

#[derive(Serialize)]
struct UserInfo {
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
