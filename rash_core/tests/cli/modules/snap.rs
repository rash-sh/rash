use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

fn get_mocks_dir() -> String {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    mocks_dir.to_str().unwrap().to_string()
}

#[test]
fn test_snap_present() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: {}/snap.rh
    name:
      - firefox
      - code
      - slack
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ firefox"));
    assert!(!stdout.contains("+ code"));
    assert!(!stdout.contains("+ slack"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_remove() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: {}/snap.rh
    name:
      - code
      - slack
      - nonexistent-snap
    state: absent
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- code"));
    assert!(stdout.contains("- slack"));
    assert!(!stdout.contains("- nonexistent-snap"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_no_change() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: {}/snap.rh
    name:
      - code
      - slack
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_snap_result_extra() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: {}/snap.rh
    name:
      - code
      - slack
      - firefox
    state: present
  register: packages
- debug:
    msg: "{{{{ packages.extra }}}}"
        "#,
        mocks_dir
    );
    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert_eq!(
        stdout.lines().last().unwrap().replace(' ', ""),
        serde_json::to_string(&json!({
            "installed_packages": ["firefox"],
            "removed_packages": [],
        }))
        .unwrap()
    );
}

#[test]
fn test_snap_with_classic() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module with classic
  snap:
    executable: {}/snap.rh
    name: firefox
    classic: true
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ firefox"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_with_channel() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module with channel
  snap:
    executable: {}/snap.rh
    name: firefox
    channel: edge
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ firefox"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_single_package() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module with single package
  snap:
    executable: {}/snap.rh
    name: firefox
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ firefox"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_list_from_var() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  vars:
    packages:
      - firefox
      - code
      - slack
  snap:
    executable: {}/snap.rh
    name: "{{{{ packages }}}}"
    state: present
  register: result
- debug:
    msg: "{{{{ result.extra }}}}"
        "#,
        mocks_dir
    );
    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert_eq!(
        stdout.lines().last().unwrap().replace(' ', ""),
        serde_json::to_string(&json!({
            "installed_packages": ["firefox"],
            "removed_packages": [],
        }))
        .unwrap()
    );
}

#[test]
fn test_snap_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: non-existent-snap.rh
    name:
      - firefox
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.lines().last().unwrap().contains(
        "Failed to execute 'non-existent-snap.rh': No such file or directory (os error 2). The executable may not be installed or not in the PATH."
    ));
}
