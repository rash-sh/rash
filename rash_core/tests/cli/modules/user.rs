use crate::cli::modules::run_test_with_env;
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter for unique test file names
static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// Helper function to get a unique passwd file for this test
fn get_unique_passwd_file() -> String {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_test_passwd_{}", test_id)
}

#[test]
fn test_user_create() {
    let passwd_file = get_unique_passwd_file();

    // Clean up passwd file before test
    let _ = std::fs::remove_file(&passwd_file);

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module create user
  user:
    name: testuser
    state: present
    uid: 1500
    shell: /bin/bash
    comment: Test User
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate user was created in passwd file
    let passwd = std::fs::read_to_string(&passwd_file).expect("passwd file should exist");
    assert!(
        passwd.contains("testuser:x:1500:"),
        "passwd should contain testuser with uid 1500"
    );
    assert!(
        passwd.contains(":/bin/bash"),
        "passwd should contain /bin/bash shell"
    );
    assert!(
        passwd.contains(":Test User:"),
        "passwd should contain Test User comment"
    );

    // Cleanup
    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_user_create_system() {
    let passwd_file = get_unique_passwd_file();

    // Clean up passwd file before test
    let _ = std::fs::remove_file(&passwd_file);

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module create system user
  user:
    name: sysuser
    state: present
    system: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate system user was created
    let passwd = std::fs::read_to_string(&passwd_file).expect("passwd file should exist");
    assert!(
        passwd.contains("sysuser:x:999:999:"),
        "passwd should contain sysuser with uid/gid 999"
    );

    // Cleanup
    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_user_delete() {
    let passwd_file = get_unique_passwd_file();

    // Setup: Create a user first
    let _ = std::fs::remove_file(&passwd_file);
    std::fs::write(
        &passwd_file,
        "olduser:x:1001:1001::/home/olduser:/bin/bash\n",
    )
    .expect("Failed to create test passwd file");

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module delete user
  user:
    name: olduser
    state: absent
    remove: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate user was removed
    let passwd = std::fs::read_to_string(&passwd_file).expect("passwd file should exist");
    assert!(
        !passwd.contains("olduser"),
        "passwd should not contain olduser after deletion"
    );

    // Cleanup
    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_user_delete_nonexistent() {
    let passwd_file = get_unique_passwd_file();

    // Clean up passwd file before test
    let _ = std::fs::remove_file(&passwd_file);

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module delete nonexistent user
  user:
    name: nonexistent
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    // User doesn't exist, so should be "ok" not "changed"
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok', got: {}",
        stdout
    );

    // Cleanup
    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_user_with_groups() {
    let passwd_file = get_unique_passwd_file();

    // Clean up passwd file before test
    let _ = std::fs::remove_file(&passwd_file);

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module with supplementary groups
  user:
    name: testuser
    state: present
    groups:
      - docker
      - wheel
    append: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate user was created
    let passwd = std::fs::read_to_string(&passwd_file).expect("passwd file should exist");
    assert!(
        passwd.contains("testuser:x:"),
        "passwd should contain testuser"
    );

    // Cleanup
    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_user_modify() {
    let passwd_file = get_unique_passwd_file();

    // Setup: Create a user first
    let _ = std::fs::remove_file(&passwd_file);
    std::fs::write(
        &passwd_file,
        "moduser:x:1002:1002:Old Comment:/home/moduser:/bin/sh\n",
    )
    .expect("Failed to create test passwd file");

    // Set environment variable for this test

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module modify user
  user:
    name: moduser
    state: present
    uid: 1003
    shell: /bin/bash
    comment: New Comment
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    // Validate user was modified
    let passwd = std::fs::read_to_string(&passwd_file).expect("passwd file should exist");
    assert!(
        passwd.contains("moduser:x:1003:"),
        "passwd should contain moduser with updated uid"
    );
    assert!(
        passwd.contains(":/bin/bash"),
        "passwd should contain updated shell"
    );
    assert!(
        passwd.contains(":New Comment:"),
        "passwd should contain updated comment"
    );
    assert!(
        !passwd.contains("Old Comment"),
        "passwd should not contain old comment"
    );

    // Cleanup
    let _ = std::fs::remove_file(&passwd_file);
}
