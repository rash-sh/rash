use crate::cli::modules::run_test;
use std::path::Path;
use std::process::Command;

fn can_run_modprobe_tests() -> bool {
    Path::new("/proc/modules").exists()
        && Command::new("modprobe").arg("--version").output().is_ok()
}

#[test]
fn test_modprobe_load_module() {
    if !can_run_modprobe_tests() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test modprobe load module
  modprobe:
    name: dummy
    state: present
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_modprobe_with_params() {
    if !can_run_modprobe_tests() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test modprobe with params
  modprobe:
    name: dummy
    params: numdummies=1
    state: present
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_modprobe_idempotent() {
    if !can_run_modprobe_tests() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Load dummy module first time
  modprobe:
    name: dummy
    state: present

- name: Load dummy module second time
  modprobe:
    name: dummy
    state: present
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_modprobe_unload_module() {
    if !can_run_modprobe_tests() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Load dummy module
  modprobe:
    name: dummy
    state: present

- name: Unload dummy module
  modprobe:
    name: dummy
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_modprobe_check_mode() {
    if !can_run_modprobe_tests() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Test modprobe in check mode
  modprobe:
    name: dummy
    state: present
        "#
    .to_string();

    let args = ["--check", "--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}
