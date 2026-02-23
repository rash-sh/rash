use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

#[test]
fn test_dnf_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/dnf.rh
    name:
      - postgresql-server
      - nginx
      - curl
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ postgresql-server"));
    assert!(!stdout.contains("+ nginx"));
    assert!(!stdout.contains("+ curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dnf_remove() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/dnf.rh
    name:
      - vim
      - curl
      - nonexistent-pkg
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- vim"));
    assert!(stdout.contains("- curl"));
    assert!(!stdout.contains("- nonexistent-pkg"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dnf_update_cache() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/dnf.rh
    update_cache: true
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dnf_result_extra() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/dnf.rh
    name:
      - postgresql-server
      - curl
      - vim
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
            "removed_packages": ["curl", "vim"],
            "cache_updated": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_dnf_list_from_var() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  vars:
    packages:
      - postgresql-server
      - nginx
      - curl
  dnf:
    executable: {}/dnf.rh
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
            "installed_packages": ["postgresql-server"],
            "removed_packages": [],
            "cache_updated": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_dnf_executable_not_found() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/non-existent-dnf.rh
    name:
      - postgresql-server
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.lines().last().unwrap().contains("Failed to execute"));
}

#[test]
fn test_dnf_with_enablerepo() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/dnf.rh
    name:
      - nginx
    enablerepo: epel
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+ nginx"));
    assert!(stderr.is_empty());
}

#[test]
fn test_dnf_with_disablerepo() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/dnf.rh
    name:
      - postgresql-server
    disablerepo: fedora-modular
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ postgresql-server"));
    assert!(stderr.is_empty());
}

#[test]
fn test_dnf_state_installed() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module with installed state
  dnf:
    executable: {}/dnf.rh
    name:
      - postgresql-server
    state: installed
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ postgresql-server"));
    assert!(stderr.is_empty());
}

#[test]
fn test_dnf_state_removed() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module with removed state
  dnf:
    executable: {}/dnf.rh
    name:
      - vim
    state: removed
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- vim"));
    assert!(stderr.is_empty());
}
