use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

fn get_mocks_dir() -> String {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    mocks_dir.to_str().unwrap().to_string()
}

#[test]
fn test_apt_present() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module
  apt:
    executable: {}/apt-get.rh
    name:
      - postgresql-client
      - nginx
      - curl
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ postgresql-client"));
    assert!(!stdout.contains("+ nginx"));
    assert!(!stdout.contains("+ curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_remove() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module
  apt:
    executable: {}/apt-get.rh
    name:
      - vim
      - curl
      - nonexistent-pkg
    state: absent
        "#,
        mocks_dir
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
fn test_apt_update_cache() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module
  apt:
    executable: {}/apt-get.rh
    update_cache: true
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_apt_present_multiple_packages() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module with multiple packages
  apt:
    executable: {}/apt-get.rh
    name:
      - gnupg
      - lsb-release
      - nginx
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ gnupg"));
    assert!(stdout.contains("+ lsb-release"));
    assert!(!stdout.contains("+ nginx"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_purge() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module with purge
  apt:
    executable: {}/apt-get.rh
    name:
      - vim
    state: absent
    purge: true
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- vim"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_install_recommends() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module with install_recommends
  apt:
    executable: {}/apt-get.rh
    name:
      - gnupg
    state: present
    install_recommends: false
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ gnupg"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_result_extra() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module
  apt:
    executable: {}/apt-get.rh
    name:
      - gnupg
      - nginx
      - vim
    state: absent
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
            "installed_packages": [],
            "removed_packages": ["nginx", "vim"],
            "upgraded_packages": [],
            "upgraded": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_apt_list_from_var() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module
  vars:
    packages:
      - gnupg
      - nginx
      - curl
  apt:
    executable: {}/apt-get.rh
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
            "installed_packages": ["gnupg"],
            "removed_packages": [],
            "upgraded_packages": [],
            "upgraded": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_apt_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test apt module
  apt:
    executable: non-existent-apt-get.rh
    name:
      - some-nonexistent-package
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.contains(
        "Failed to execute 'non-existent-apt-get.rh': No such file or directory (os error 2). The executable may not be installed or not in the PATH."
    ));
}

#[test]
fn test_apt_no_change_when_already_installed() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test apt module
  apt:
    executable: {}/apt-get.rh
    name:
      - curl
      - nginx
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+ curl"));
    assert!(!stdout.contains("+ nginx"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}
