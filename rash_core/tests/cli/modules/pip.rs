use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

fn get_mocks_dir() -> String {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    mocks_dir.to_str().unwrap().to_string()
}

#[test]
fn test_pip_present() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip3.rh
    name:
      - pytest
      - requests
      - flask
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ pytest"));
    assert!(!stdout.contains("+ requests"));
    assert!(!stdout.contains("+ flask"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_remove() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip3.rh
    name:
      - requests
      - flask
      - nonexistent-pkg
    state: absent
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- requests"));
    assert!(stdout.contains("- flask"));
    assert!(!stdout.contains("- nonexistent-pkg"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_present_single_package() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module with single package
  pip:
    executable: {}/pip3.rh
    name: pytest
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ pytest"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_present_multiple_packages() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module with multiple packages
  pip:
    executable: {}/pip3.rh
    name:
      - pytest
      - black
      - mypy
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ pytest"));
    assert!(stdout.contains("+ black"));
    assert!(stdout.contains("+ mypy"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_version() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module with version
  pip:
    executable: {}/pip3.rh
    name: pytest
    version: "7.0.0"
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ pytest==7.0.0"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_result_extra() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip3.rh
    name:
      - pytest
      - requests
      - flask
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
            "removed_packages": ["requests", "flask"],
            "requirements_installed": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_pip_list_from_var() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  vars:
    packages:
      - pytest
      - requests
      - flask
  pip:
    executable: {}/pip3.rh
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
            "installed_packages": ["pytest"],
            "removed_packages": [],
            "requirements_installed": false,
        }))
        .unwrap()
    );
}

#[test]
fn test_pip_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: non-existent-pip.rh
    name:
      - requests
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.lines().last().unwrap().contains(
        "Failed to execute 'non-existent-pip.rh': No such file or directory (os error 2). The executable may not be installed or not in the PATH."
    ));
}

#[test]
fn test_pip_no_change_when_already_installed() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip3.rh
    name:
      - requests
      - flask
    state: present
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+ requests"));
    assert!(!stdout.contains("+ flask"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_pip_forcereinstall() {
    let mocks_dir = get_mocks_dir();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module with forcereinstall
  pip:
    executable: {}/pip3.rh
    name:
      - requests
    state: forcereinstall
        "#,
        mocks_dir
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ requests"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}
