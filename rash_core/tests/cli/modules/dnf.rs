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
      - nginx
      - vim-enhanced
      - curl
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ nginx"));
    assert!(!stdout.contains("+ vim-enhanced"));
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
      - vim-enhanced
      - curl
      - nonexistent-pkg
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- vim-enhanced"));
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
fn test_dnf_latest() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: {}/dnf.rh
    name:
      - curl
      - nginx
    state: latest
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ nginx"));
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
      - nginx
      - curl
      - vim-enhanced
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
            "removed_packages": ["curl", "vim-enhanced"],
            "upgraded_packages": [],
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
      - nginx
      - curl
      - vim-enhanced
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
            "installed_packages": ["nginx"],
            "removed_packages": [],
            "upgraded_packages": [],
            "cache_updated": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_dnf_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test dnf module
  dnf:
    executable: non-existent-dnf.rh
    name:
      - curl
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.lines().last().unwrap().contains(
        "Failed to execute 'non-existent-dnf.rh': No such file or directory (os error 2). The executable may not be installed or not in the PATH."
    ));
}

#[test]
fn test_dnf_with_repo_options() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test dnf module with repo options
  dnf:
    executable: {}/dnf.rh
    name: nginx
    state: present
    enablerepo: epel
    disablerepo: updates
    disable_gpg_check: true
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ nginx"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}
