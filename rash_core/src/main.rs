//#![deny(warnings)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_json;

mod constants;
mod context;
mod error;
mod facts;
mod input;
mod logger;
mod modules;
mod task;

use std::path::PathBuf;

lazy_static! {
    static ref TASKS_PATH: PathBuf = PathBuf::from("./entrypoint.rh");
}

fn main() {
    logger::init();
    debug!("start logger");
    println!("TODO");
}

#[cfg(test)]
mod tests {
    use super::*;

    use context::Context;
    use error::ErrorKind;
    use facts::FACTS_SOURCES;
    use input::read_file;

    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_command_ls() {
        logger::init();
        let facts_fn = FACTS_SOURCES.get("env").unwrap();
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("entrypoint.rh");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(
            file,
            r#"
        #!/bin/rash
        - name: test ls
          command: ls

        - command:
            cmd: ls /
        "#
        )
        .unwrap();

        let context = Context::new(read_file(file_path).unwrap(), (facts_fn)().unwrap());
        let context_error = Context::exec(context).unwrap_err();
        let _ = match context_error.kind() {
            ErrorKind::EmptyTaskStack => (),
            _ => panic!(context_error),
        };
    }
}
