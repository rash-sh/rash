mod include;
mod pacman;
mod systemd;

use super::execute_rash;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

use tempfile::tempdir;

pub fn run_tests(
    scripts: HashMap<&str, &str>,
    entrypoint: &str,
    args: &[&str],
) -> (String, String) {
    let tmp_dir = tempdir().unwrap();

    scripts.into_iter().for_each(|(name, content)| {
        let script_path = tmp_dir.path().join(name);
        let mut script_file = File::create(&script_path).unwrap();
        script_file.write_all(content.as_bytes()).unwrap();
    });

    let entrypoint_path = tmp_dir.path().join(entrypoint);
    let mut args_with_entrypoint = args.to_vec();
    args_with_entrypoint.push(entrypoint_path.to_str().unwrap());

    execute_rash(&args_with_entrypoint)
}

pub fn run_test(content: &str, args: &[&str]) -> (String, String) {
    let entrypoint = "script.rh";
    let scripts = HashMap::from([(entrypoint, content)]);
    run_tests(scripts, entrypoint, args)
}
