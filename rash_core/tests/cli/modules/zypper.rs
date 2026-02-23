use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

#[test]
fn test_zypper_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test zypper module
  zypper:
    executable: {}/zypper.rh
    name:
      - jq
      - postgresql-client
      - curl
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ jq"));
    assert!(stdout.contains("+ postgresql-client"));
    assert!(!stdout.contains("+ curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_zypper_remove() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test zypper module
  zypper:
    executable: {}/zypper.rh
    name:
      - nonexistent-pkg
      - curl
      - vim
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- curl"));
    assert!(stdout.contains("- vim"));
    assert!(!stdout.contains("- nonexistent-pkg"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_zypper_update_cache() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test zypper module
  zypper:
    executable: {}/zypper.rh
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
fn test_zypper_result_extra() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test zypper module
  zypper:
    executable: {}/zypper.rh
    name:
      - jq
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
fn test_zypper_list_from_var() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test zypper module
  vars:
    packages:
      - jq
      - postgresql-client
      - curl
  zypper:
    executable: {}/zypper.rh
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
            "installed_packages": ["jq", "postgresql-client"],
            "removed_packages": [],
            "cache_updated": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_zypper_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test zypper module
  zypper:
    executable: non-existent-zypper.rh
    name:
      - curl
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.lines().last().unwrap().contains(
        "Failed to execute 'non-existent-zypper.rh': No such file or directory (os error 2). The executable may not be installed or not in the PATH."
    ));
}
