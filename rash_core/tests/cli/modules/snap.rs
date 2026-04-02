use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

#[test]
fn test_snap_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: {}/snap.rh
    name:
      - chromium
      - hello-world
      - curl
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ chromium"));
    assert!(stdout.contains("+ hello-world"));
    assert!(!stdout.contains("+ curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_remove() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: {}/snap.rh
    name:
      - vlc
      - curl
      - nonexistent-pkg
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- vlc"));
    assert!(stdout.contains("- curl"));
    assert!(!stdout.contains("- nonexistent-pkg"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_classic() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module with classic
  snap:
    executable: {}/snap.rh
    name: code
    classic: true
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ code"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_channel() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module with channel
  snap:
    executable: {}/snap.rh
    name: lxd
    channel: edge
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ lxd"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_snap_result_extra() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  snap:
    executable: {}/snap.rh
    name:
      - chromium
      - curl
      - vlc
    state: absent
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
            "installed_packages": [],
            "removed_packages": ["curl", "vlc"],
        }))
        .unwrap()
    );
}

#[test]
fn test_snap_list_from_var() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test snap module
  vars:
    packages:
      - chromium
      - hello-world
      - curl
  snap:
    executable: {}/snap.rh
    name: "{{{{ packages }}}}"
    state: present
  register: result
- debug:
    msg: "{{{{ result.extra }}}}"
        "#,
        mocks_dir.to_str().unwrap()
    );
    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert_eq!(
        stdout.lines().last().unwrap().replace(' ', ""),
        serde_json::to_string(&json!({
            "installed_packages": ["chromium", "hello-world"],
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
      - curl
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.lines().last().unwrap().contains(
        "Failed to execute 'non-existent-snap.rh': No such file or directory (os error 2). The executable may not be installed or not in the PATH."
    ));
}
