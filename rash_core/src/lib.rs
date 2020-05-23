pub mod context;
pub mod error;
pub mod facts;
pub mod logger;
pub mod modules;
pub mod task;
pub mod utils;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_json;

#[cfg(test)]
mod tests {
    use super::*;

    use context::Context;
    use error::ErrorKind;
    use facts::env;
    use task::read_file;

    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_command_ls() {
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

        let context = Context::new(read_file(file_path).unwrap(), env::load(vec![]).unwrap());
        let context_error = Context::exec(context).unwrap_err();
        let _ = match context_error.kind() {
            ErrorKind::EmptyTaskStack => (),
            _ => panic!(context_error),
        };
    }
}
