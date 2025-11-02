use crate::cli::modules::run_test_with_env;
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter for unique test file names
static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// Helper function to get a unique group file for this test
fn get_unique_group_file() -> String {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_test_group_{}", test_id)
}

#[test]
fn test_group_create() {
    let group_file = get_unique_group_file();

    // Clean up group file before test
    let _ = std::fs::remove_file(&group_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test group module create group
  group:
    name: testgroup
    state: present
    gid: 1500
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_GROUP_FILE", &group_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate group was created in group file
    let groupfile = std::fs::read_to_string(&group_file).expect("group file should exist");
    assert!(
        groupfile.contains("testgroup:x:1500:"),
        "group should contain testgroup with gid 1500"
    );

    // Cleanup
    let _ = std::fs::remove_file(&group_file);
}

#[test]
fn test_group_create_system() {
    let group_file = get_unique_group_file();

    // Clean up group file before test
    let _ = std::fs::remove_file(&group_file);

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test group module create system group
  group:
    name: sysgroup
    state: present
    system: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_GROUP_FILE", &group_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate system group was created
    let groupfile = std::fs::read_to_string(&group_file).expect("group file should exist");
    assert!(
        groupfile.contains("sysgroup:x:999:"),
        "group should contain sysgroup with gid 999"
    );

    // Cleanup
    let _ = std::fs::remove_file(&group_file);
}

#[test]
fn test_group_delete() {
    let group_file = get_unique_group_file();

    // Setup: Create a group first
    let _ = std::fs::remove_file(&group_file);
    std::fs::write(&group_file, "oldgroup:x:1001:\n").expect("Failed to create test group file");

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test group module delete group
  group:
    name: oldgroup
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_GROUP_FILE", &group_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate group was removed
    let groupfile = std::fs::read_to_string(&group_file).expect("group file should exist");
    assert!(
        !groupfile.contains("oldgroup"),
        "group should not contain oldgroup after deletion"
    );

    // Cleanup
    let _ = std::fs::remove_file(&group_file);
}

#[test]
fn test_group_delete_nonexistent() {
    let group_file = get_unique_group_file();

    // Clean up group file before test
    let _ = std::fs::remove_file(&group_file);

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test group module delete nonexistent group
  group:
    name: nonexistent
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_GROUP_FILE", &group_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    // Group doesn't exist, so should be "ok" not "changed"
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok', got: {}",
        stdout
    );

    // Cleanup
    let _ = std::fs::remove_file(&group_file);
}

#[test]
fn test_group_modify() {
    let group_file = get_unique_group_file();

    // Setup: Create a group first
    let _ = std::fs::remove_file(&group_file);
    std::fs::write(&group_file, "modgroup:x:1002:\n").expect("Failed to create test group file");

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test group module modify group
  group:
    name: modgroup
    state: present
    gid: 1003
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_GROUP_FILE", &group_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate group was modified
    let groupfile = std::fs::read_to_string(&group_file).expect("group file should exist");
    assert!(
        groupfile.contains("modgroup:x:1003:"),
        "group should contain modgroup with updated gid"
    );
    assert!(
        !groupfile.contains("modgroup:x:1002:"),
        "group should not contain old gid"
    );

    // Cleanup
    let _ = std::fs::remove_file(&group_file);
}

#[test]
fn test_group_idempotent() {
    let group_file = get_unique_group_file();

    // Setup: Create a group first
    let _ = std::fs::remove_file(&group_file);
    std::fs::write(&group_file, "idempgroup:x:1004:\n").expect("Failed to create test group file");

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test group module idempotency
  group:
    name: idempgroup
    state: present
    gid: 1004
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_GROUP_FILE", &group_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    // Group already exists with same gid, so should be "ok" not "changed"
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok' for idempotent operation, got: {}",
        stdout
    );

    // Cleanup
    let _ = std::fs::remove_file(&group_file);
}
