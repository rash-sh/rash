use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

fn get_mocks_dir() -> String {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    mocks_dir.to_str().unwrap().to_string()
}

#[test]
fn test_apt_hold_single_package() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold curl package
  apt_hold:
    executable: {}
    name: curl
        "#,
        apt_mark_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_hold_multiple_packages() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold multiple packages
  apt_hold:
    executable: {}
    name:
      - curl
      - vim
      - bash
        "#,
        apt_mark_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("curl"));
    assert!(stdout.contains("vim"));
    assert!(stdout.contains("bash"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_hold_already_held() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold nginx package (already held)
  apt_hold:
    executable: {}
    name: nginx
        "#,
        apt_mark_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("nginx"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_apt_hold_explicit_state() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold package with explicit state
  apt_hold:
    executable: {}
    name: curl
    state: held
        "#,
        apt_mark_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_hold_unhold() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Unhold nginx package
  apt_hold:
    executable: {}
    name: nginx
    state: unheld
        "#,
        apt_mark_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("nginx"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_hold_unhold_not_held() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Unhold curl package (not held)
  apt_hold:
    executable: {}
    name: curl
    state: unheld
        "#,
        apt_mark_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_apt_hold_unhold_multiple() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Unhold multiple packages
  apt_hold:
    executable: {}
    name:
      - nginx
      - docker-ce
      - curl
    state: unheld
        "#,
        apt_mark_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("nginx"));
    assert!(stdout.contains("docker-ce"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_apt_hold_result_extra() {
    let mocks_dir = get_mocks_dir();
    let apt_mark_path = format!("{}/apt-mark", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold package
  apt_hold:
    executable: {}
    name: curl
  register: result
- debug:
    msg: "{{{{ result.extra }}}}"
        "#,
        apt_mark_path
    );

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let last_line = stdout.lines().last().unwrap().replace(' ', "");
    assert!(last_line.contains("curl"));
    assert!(last_line.contains("hold"));
}

#[test]
fn test_apt_hold_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test with non-existent executable
  apt_hold:
    executable: non-existent-apt-mark
    name: some-package
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.contains("Failed to execute"));
}
