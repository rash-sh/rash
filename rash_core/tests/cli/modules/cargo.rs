use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

#[test]
fn test_cargo_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test cargo module
  cargo:
    executable: {}/cargo.rh
    name:
      - ripgrep
      - fd-find
      - bat
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_cargo_install_new() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test cargo module
  cargo:
    executable: {}/cargo.rh
    name:
      - tokei
      - exa
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ tokei"));
    assert!(stdout.contains("+ exa"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_cargo_remove() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test cargo module
  cargo:
    executable: {}/cargo.rh
    name:
      - ripgrep
      - nonexistent-crate
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- ripgrep"));
    assert!(!stdout.contains("- nonexistent-crate"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_cargo_latest() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test cargo module
  cargo:
    executable: {}/cargo.rh
    name:
      - ripgrep
      - tokei
    state: latest
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ tokei"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_cargo_result_extra() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test cargo module
  cargo:
    executable: {}/cargo.rh
    name:
      - ripgrep
      - tokei
      - bat
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
            "installed_crates": [],
            "removed_crates": ["bat", "ripgrep"],
        }))
        .unwrap()
    );
}

#[test]
fn test_cargo_list_from_var() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test cargo module
  vars:
    crates:
      - tokei
      - exa
  cargo:
    executable: {}/cargo.rh
    name: "{{{{ crates }}}}"
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
            "installed_crates": ["exa", "tokei"],
            "removed_crates": [],
        }))
        .unwrap()
    );
}

#[test]
fn test_cargo_executable_not_found() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test cargo module
  cargo:
    executable: non-existent-cargo.rh
    name:
      - ripgrep
    state: present
        "#
    .to_string();
    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(stderr.lines().last().unwrap().contains(
        "Failed to execute 'non-existent-cargo.rh': No such file or directory (os error 2). The executable may not be installed or not in the PATH."
    ));
}
