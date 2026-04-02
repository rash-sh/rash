use crate::cli::modules::run_test;

#[test]
fn test_swapfile_check_mode_create() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create swap file in check mode
  swapfile:
    path: /tmp/test_swapfile
    size: 1M
    state: created
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
#[cfg(target_os = "linux")]
fn test_swapfile_check_mode_absent() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Remove nonexistent swap file
  swapfile:
    path: /tmp/nonexistent_swapfile
    state: absent
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("ok"));
}

#[test]
#[cfg(target_os = "linux")]
fn test_swapfile_disabled_state() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Disable nonexistent swap
  swapfile:
    path: /tmp/nonexistent_swapfile
    state: disabled
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("ok"));
}

#[test]
fn test_swapfile_invalid_size_missing() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create swap without size
  swapfile:
    path: /tmp/test_swapfile
    state: present
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("size parameter is required"));
}

#[test]
fn test_swapfile_invalid_priority() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create swap with invalid priority
  swapfile:
    path: /tmp/test_swapfile
    size: 1M
    priority: 50000
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("priority must be between"));
}

#[test]
fn test_swapfile_created_state() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create swap file without enabling
  swapfile:
    path: /tmp/test_swapfile_created
    size: 512K
    state: created
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}
