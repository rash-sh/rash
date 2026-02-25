use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

#[test]
fn test_pip_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip.rh
    name: mypackage
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ mypackage"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_absent() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip.rh
    name: django
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- django"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_present_already_installed() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip.rh
    name: requests
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+ requests"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_pip_multiple_packages() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip.rh
    name:
      - mypackage1
      - mypackage2
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ mypackage1"));
    assert!(stdout.contains("+ mypackage2"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_requirements() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: {}/pip.rh
    requirements: /my_app/requirements.txt
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ /my_app/requirements.txt"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_pip_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    executable: non-existent-pip.rh
    name: bottle
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
fn test_pip_missing_name_and_requirements() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.contains("one of 'name' or 'requirements' is required"));
}

#[test]
fn test_pip_executable_and_virtualenv_mutually_exclusive() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test pip module
  pip:
    name: bottle
    executable: pip3
    virtualenv: /my_app/venv
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.contains("'executable' and 'virtualenv' are mutually exclusive"));
}
