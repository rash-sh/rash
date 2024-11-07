use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

#[test]
fn test_pacman_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    force: true
    name:
      - rustup
      - bpftrace
      - linux61-zfs
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ rustup"));
    assert!(stdout.contains("+ bpftrace"));
    assert!(!stdout.contains("+ linux61-zfs"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pacman_remove() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    force: true
    name:
      - linux61-nvidia
      - linux61-zfs
      - rash
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- linux61-nvidia"));
    assert!(stdout.contains("- linux61-zfs"));
    assert!(!stdout.contains("- rash"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pacman_sync() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    name:
      - linux61-nvidia
      - linux61-zfs
      - rash
    state: sync
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- linux-firmware"));
    assert!(stdout.contains("- linux61"));
    assert!(stdout.contains("+ rash"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pacman_sync_upgrade_no_changed() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    upgrade: true
    name:
      - linux-firmware
      - linux61
      - linux61-nvidia
      - linux61-zfs
    state: sync
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+ linux-firmware"));
    assert!(!stdout.contains("+ linux61"));
    assert!(!stdout.contains("+ linux61-nvidia"));
    assert!(!stdout.contains("+ linux61-zfs"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_pacman_result_extra() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    upgrade: true
    name:
      - linux-firmware
      - linux61
      - linux61-nvidia
      - rash
    state: sync
  register: packages
- debug:
    msg: "{{{{ packages.extra }}}}"
        "#,
        mocks_dir.to_str().unwrap()
    );
    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert_eq!(
        stdout.lines().last().unwrap().replace(' ', ""),
        serde_json::to_string(&json!({
            "installed_packages": ["rash"],
            "removed_packages": ["linux61-zfs"],
            "upgraded": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_pacman_list_from_var() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  vars:
    packages:
      - linux-firmware
      - linux61
      - linux61-nvidia
      - rash
  pacman:
    executable: {}/pacman.rh
    upgrade: true
    name: "{{{{ packages }}}}"
    state: sync
  register: packages
- debug:
    msg: "{{{{ packages.extra }}}}"
        "#,
        mocks_dir.to_str().unwrap()
    );
    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert_eq!(
        stdout.lines().last().unwrap().replace(' ', ""),
        serde_json::to_string(&json!({
            "installed_packages": ["rash"],
            "removed_packages": ["linux61-zfs"],
            "upgraded": false,
        }))
        .unwrap()
    );

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
vars:
  packages:
    - linux-firmware
    - linux61
    - linux61-nvidia
    - rash
pacman:
  executable: {}/pacman.rh
  upgrade: true
  name:
    - "{{{{ packages }}}}"
  state: sync
      "#,
        mocks_dir.to_str().unwrap()
    );
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}
