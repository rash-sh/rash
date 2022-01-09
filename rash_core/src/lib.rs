pub mod context;
pub mod docopt;
pub mod error;
pub mod logger;
pub mod modules;
pub mod task;
pub mod utils;
pub mod vars;

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
    use task::parse_file;
    use vars::env;

    #[test]
    fn test_command_ls() {
        let file = r#"
        #!/bin/rash
        - name: test ls
          command: ls

        - command:
            cmd: ls /
        "#;

        let context = Context::new(
            parse_file(&file, &task::GlobalParams::default()).unwrap(),
            env::load(vec![]).unwrap(),
        );
        let context_error = Context::exec(context).unwrap_err();

        let _ = match context_error.kind() {
            ErrorKind::EmptyTaskStack => (),
            _ => panic!("{}", context_error),
        };
    }
}
