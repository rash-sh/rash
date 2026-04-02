use std::env;
use std::path::Path;

use crate::cli::modules::run_test;

use serde_json::json;

#[test]
fn test_flatpak_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test flatpak module
  flatpak:
    executable: {}/flatpak.rh
    name:
      - org.gnome.Calendar
      - org.gnome.Todo
      - org.gnome.Epiphany
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+ org.gnome.Calendar"));
    assert!(!stdout.contains("+ org.gnome.Todo"));
    assert!(stdout.contains("+ org.gnome.Epiphany"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_flatpak_remove() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test flatpak module
  flatpak:
    executable: {}/flatpak.rh
    name:
      - org.gnome.Calendar
      - org.gnome.Todo
      - org.gnome.Epiphany
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("- org.gnome.Calendar"));
    assert!(stdout.contains("- org.gnome.Todo"));
    assert!(!stdout.contains("- org.gnome.Epiphany"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_flatpak_no_change() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test flatpak module
  flatpak:
    executable: {}/flatpak.rh
    name:
      - org.gnome.Calendar
      - org.gnome.Todo
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("+"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_flatpak_result_extra() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test flatpak module
  flatpak:
    executable: {}/flatpak.rh
    name:
      - org.gnome.Calendar
      - org.gnome.Todo
      - org.gnome.Epiphany
    state: present
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
            "installed_packages": ["org.gnome.Epiphany"],
            "removed_packages": [],
        }))
        .unwrap()
    );
}

#[test]
fn test_flatpak_with_remote() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test flatpak module
  flatpak:
    executable: {}/flatpak.rh
    name: org.gnome.Epiphany
    remote: flathub
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ org.gnome.Epiphany"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_flatpak_with_method_user() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test flatpak module
  flatpak:
    executable: {}/flatpak.rh
    name: org.gnome.Epiphany
    method: user
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ org.gnome.Epiphany"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}

#[test]
fn test_flatpak_with_no_deps() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test flatpak module
  flatpak:
    executable: {}/flatpak.rh
    name: org.gnome.Epiphany
    no_deps: true
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("+ org.gnome.Epiphany"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("changed\n"));
}
