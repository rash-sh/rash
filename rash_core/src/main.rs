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
mod logger;
mod modules;
mod plugins;
mod task;

use context::Context;
use plugins::facts::FACTS_SOURCES;

use std::path::PathBuf;

lazy_static! {
    static ref TASKS_PATH: PathBuf = PathBuf::from("./entrypoint.rh");
}

fn main() {
    logger::init();
    debug!("start logger");
    let facts_fn = FACTS_SOURCES.get("env").expect("Inventory does not exists");
    let context = Context::new(
        TASKS_PATH.to_path_buf(),
        (facts_fn)().expect("Failed to load inventory"),
    )
    .expect("Failed to load context");
    let _ = context.execute_task().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_command_ls() {
        logger::init();
        let facts_fn = FACTS_SOURCES.get("env").expect("Inventory does not exists");
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

        let context =
            Context::new(file_path, (facts_fn)().unwrap()).expect("Failed to load context");
        let _ = context.execute_task().unwrap();
    }
}
