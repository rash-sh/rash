use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

fn get_mocks_dir() -> String {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    mocks_dir.to_str().unwrap().to_string()
}

#[test]
fn test_dpkg_selections_hold() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold curl package
  dpkg_selections:
    executable: {}
    name: curl
    selection: hold
        "#,
        dpkg_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dpkg_selections_hold_multiple() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold multiple packages
  dpkg_selections:
    executable: {}
    name:
      - curl
      - vim
      - bash
    selection: hold
        "#,
        dpkg_path
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
fn test_dpkg_selections_no_change_already_hold() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold nginx package (already held)
  dpkg_selections:
    executable: {}
    name: nginx
    selection: hold
        "#,
        dpkg_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("nginx"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_dpkg_selections_unhold() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Unhold nginx package
  dpkg_selections:
    executable: {}
    name: nginx
    selection: install
        "#,
        dpkg_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("nginx"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dpkg_selections_query() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Query nginx selection
  dpkg_selections:
    executable: {}
    name: nginx
  register: nginx_status
- debug:
    msg: "{{{{ nginx_status.extra }}}}"
        "#,
        dpkg_path
    );

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let last_line = stdout.lines().last().unwrap().replace(' ', "");
    assert!(last_line.contains("nginx"));
}

#[test]
fn test_dpkg_selections_result_extra() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold package
  dpkg_selections:
    executable: {}
    name: curl
    selection: hold
  register: result
- debug:
    msg: "{{{{ result.extra }}}}"
        "#,
        dpkg_path
    );

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let last_line = stdout.lines().last().unwrap().replace(' ', "");
    assert!(last_line.contains("curl"));
    assert!(last_line.contains("hold"));
}

#[test]
fn test_dpkg_selections_deinstall() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Mark package for deinstall
  dpkg_selections:
    executable: {}
    name: vim
    selection: deinstall
        "#,
        dpkg_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("vim"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dpkg_selections_purge() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Mark package for purge
  dpkg_selections:
    executable: {}
    name: vim
    selection: purge
        "#,
        dpkg_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("vim"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dpkg_selections_check_mode() {
    let mocks_dir = get_mocks_dir();
    let dpkg_path = format!("{}/dpkg", mocks_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Hold package in check mode
  dpkg_selections:
    executable: {}
    name: curl
    selection: hold
        "#,
        dpkg_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ curl"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_dpkg_selections_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test with non-existent executable
  dpkg_selections:
    executable: non-existent-dpkg
    name: some-package
    selection: hold
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.contains("Failed to execute"));
}
