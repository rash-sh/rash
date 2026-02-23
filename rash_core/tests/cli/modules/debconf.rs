use crate::cli::modules::run_test_with_env;
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

fn get_test_state_file() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_debconf_mock_state_{}", id)
}

fn cleanup_state_file(path: &str) {
    let _ = std::fs::remove_file(path);
}

fn build_test_path() -> String {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    let target_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/debug");
    format!(
        "{}:{}:{}",
        mocks_dir.to_str().unwrap(),
        target_dir.to_str().unwrap(),
        env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn test_debconf_set_value() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: Set MySQL root password
  debconf:
    name: mysql-server
    question: mysql-server/root_password
    value: secret
    vtype: password
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("PATH", &mock_path),
            ("DEBCONF_MOCK_STATE_FILE", &state_file),
        ],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Set question 'mysql-server/root_password'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}

#[test]
fn test_debconf_set_value_string() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: Set keyboard layout
  debconf:
    name: keyboard-configuration
    question: keyboard-configuration/layoutcode
    value: us
    vtype: select
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("PATH", &mock_path),
            ("DEBCONF_MOCK_STATE_FILE", &state_file),
        ],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Set question 'keyboard-configuration/layoutcode'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}

#[test]
fn test_debconf_set_value_with_unseen() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: Set timezone with unseen
  debconf:
    name: tzdata
    question: tzdata/Areas
    value: Etc
    vtype: select
    unseen: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("PATH", &mock_path),
            ("DEBCONF_MOCK_STATE_FILE", &state_file),
        ],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Set question 'tzdata/Areas'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}

#[test]
fn test_debconf_check_mode() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: Check mode test
  debconf:
    name: mysql-server
    question: mysql-server/root_password
    value: secret
    vtype: password
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("PATH", &mock_path),
            ("DEBCONF_MOCK_STATE_FILE", &state_file),
        ],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Would set question"), "stdout: {}", stdout);

    cleanup_state_file(&state_file);
}

#[test]
fn test_debconf_boolean_type() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: Set boolean value
  debconf:
    name: some-package
    question: some-package/enable-feature
    value: "true"
    vtype: boolean
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("PATH", &mock_path),
            ("DEBCONF_MOCK_STATE_FILE", &state_file),
        ],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Set question 'some-package/enable-feature'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}

#[test]
fn test_debconf_default_vtype() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: Set value with default vtype
  debconf:
    name: some-package
    question: some-package/some-setting
    value: "some value"
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("PATH", &mock_path),
            ("DEBCONF_MOCK_STATE_FILE", &state_file),
        ],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Set question 'some-package/some-setting'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}
