mod apk;
mod apt;
mod authorized_key;
mod cargo;
mod conntrack;
mod cron;
mod cronvar;
mod crypttab;
mod dconf;
mod dnf;
mod docker_compose;
mod docker_config;
mod docker_container;
mod docker_image;
mod docker_info;
mod docker_login;
mod docker_network;
mod docker_prune;
mod docker_volume;
mod dpkg_selections;
mod fail;
mod fail2ban;
mod firewalld;
mod flatpak;
mod gem;
mod git;
mod group;
mod grub;
mod haproxy;
mod hostname;
mod htpasswd;
mod include;
mod incus;
mod ipaddr;
mod kernel_blacklist;
mod kubectl;
mod logrotate;
mod luks;
mod modprobe;
mod netplan;
mod npm;
mod openrc;
mod pacman;
mod pam_limits;
mod patch;
mod pids;
mod pip;
mod rclone;
mod reboot;
mod replace;
mod restic;
mod runit;
mod seboolean;
mod ssh_config;
mod sshd_config;
mod sudoers;
mod swapfile;
mod syslog;
mod systemd;
mod tailscale;
mod timezone;
mod trace;
mod ufw;
mod user;
mod wakeonlan;
mod xattr;
mod zypper;

use super::execute_rash_with_env;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, MutexGuard, OnceLock};

use tempfile::tempdir;

static DOCKER_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn docker_test_lock() -> MutexGuard<'static, ()> {
    DOCKER_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

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
