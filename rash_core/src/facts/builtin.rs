use crate::error::{Error, ErrorKind, Result};

use std::path::Path;

use nix::unistd::{getuid, User};
use serde::Serialize;

#[derive(Serialize)]
struct UserInfo {
    name: String,
    uid: u32,
    gid: u32,
}

#[derive(Serialize)]
pub struct Builtins<P: AsRef<Path>> {
    args: Vec<String>,
    dir: P,
    user: UserInfo,
}

impl<P: AsRef<Path>> Builtins<P> {
    pub fn new(args: Vec<&str>, dir: P) -> Result<Self> {
        let user = User::from_uid(getuid())
            .or_else(|e| Err(Error::new(ErrorKind::Other, e)))?
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "No user with current uid"))?;
        Ok(Builtins {
            args: args.into_iter().map(String::from).collect(),
            dir,
            user: UserInfo {
                name: user.name,
                uid: user.uid.as_raw(),
                gid: user.gid.as_raw(),
            },
        })
    }
}
