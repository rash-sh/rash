use crate::cli::modules::run_test_with_env;
use std::env;
use std::path::Path;

// Generate a unique state file path for each test
fn get_test_state_file() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_dconf_mock_state_{}", id)
}

// Clean up the mock state file
fn cleanup_state_file(path: &str) {
    let _ = std::fs::remove_file(path);
}

// Build PATH that includes both mocks directory and rash binary
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
fn test_dconf_write_value() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module write value
  dconf:
    key: "/org/gnome/desktop/interface/clock-format"
    value: "'12h'"
    state: present
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Set key '/org/gnome/desktop/interface/clock-format' to '12h'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_write_value_already_set() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module write value (first time)
  dconf:
    key: "/org/gnome/desktop/interface/gtk-theme"
    value: "'Adwaita'"
    state: present

- name: test dconf module write value (already set)
  dconf:
    key: "/org/gnome/desktop/interface/gtk-theme"
    value: "'Adwaita'"
    state: present
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("already set"), "stdout: {}", stdout);

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_read_value() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module read value
  dconf:
    key: "/org/gnome/desktop/interface/clock-format"
    state: read
  register: clock_format

- debug:
    msg: "{{ clock_format.extra.value }}"
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Key '/org/gnome/desktop/interface/clock-format' = '24h'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_reset_value() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module reset value
  dconf:
    key: "/org/gnome/desktop/interface/icon-theme"
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Reset key '/org/gnome/desktop/interface/icon-theme'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_reset_already_reset() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module reset value (first time)
  dconf:
    key: "/org/gnome/desktop/interface/text-scaling-factor"
    state: absent

- name: test dconf module reset value (already reset)
  dconf:
    key: "/org/gnome/desktop/interface/text-scaling-factor"
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("already not set"), "stdout: {}", stdout);

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_default_state_present() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module default state (present)
  dconf:
    key: "/org/gnome/desktop/interface/enable-animations"
    value: "true"
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Set key"), "stdout: {}", stdout);

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_error_missing_value_for_present() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module error when value missing for present state
  dconf:
    key: "/org/gnome/desktop/interface/clock-format"
    state: present
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(!stderr.is_empty());
    assert!(stderr.contains("value is required when state is present"));

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_check_mode() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module in check mode
  dconf:
    key: "/org/gnome/desktop/interface/clock-show-date"
    value: "true"
    state: present
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Would set key"), "stdout: {}", stdout);

    cleanup_state_file(&state_file);
}

#[test]
fn test_dconf_array_value() {
    let state_file = get_test_state_file();
    let mock_path = build_test_path();

    let script_text = r#"
#!/usr/bin/env rash
- name: test dconf module with array value
  dconf:
    key: "/org/gnome/desktop/input-sources/sources"
    value: "[('xkb', 'us'), ('xkb', 'se')]"
    state: present
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("PATH", &mock_path), ("DCONF_MOCK_STATE_FILE", &state_file)],
    );

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("Set key '/org/gnome/desktop/input-sources/sources'"),
        "stdout: {}",
        stdout
    );

    cleanup_state_file(&state_file);
}
