mod apk;
mod authorized_key;
mod cargo;
mod cron;
mod dconf;
mod dnf;
mod fail;
mod firewalld;
mod gem;
mod group;
mod hostname;
mod include;
mod kernel_blacklist;
mod logrotate;
mod pacman;
mod pam_limits;
mod reboot;
mod seboolean;
mod systemd;
mod timezone;
mod trace;
mod user;
mod zypper;

use super::execute_rash_with_env;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

use tempfile::tempdir;

pub fn run_tests(
    scripts: HashMap<&str, &str>,
    entrypoint: &str,
    args: &[&str],
) -> (String, String) {
    run_tests_with_env(scripts, entrypoint, args, &[])
}

pub fn run_tests_with_env(
    scripts: HashMap<&str, &str>,
    entrypoint: &str,
    args: &[&str],
    env_vars: &[(&str, &str)],
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

    execute_rash_with_env(&args_with_entrypoint, env_vars)
}

pub fn run_test(content: &str, args: &[&str]) -> (String, String) {
    run_test_with_env(content, args, &[])
}

pub fn run_test_with_env(
    content: &str,
    args: &[&str],
    env_vars: &[(&str, &str)],
) -> (String, String) {
    let entrypoint = "script.rh";
    let scripts = HashMap::from([(entrypoint, content)]);
    run_tests_with_env(scripts, entrypoint, args, env_vars)
}
