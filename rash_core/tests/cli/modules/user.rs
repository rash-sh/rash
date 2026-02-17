use crate::cli::modules::run_test_with_env;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_temp_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("Failed to write content");
    file
}

#[test]
fn test_user_create() {
    let passwd_file = create_temp_file("");

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
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_path)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let passwd = std::fs::read_to_string(&passwd_path).expect("passwd file should exist");
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
}

#[test]
fn test_user_create_system() {
    let passwd_file = create_temp_file("");

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
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_path)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let passwd = std::fs::read_to_string(&passwd_path).expect("passwd file should exist");
    assert!(
        passwd.contains("sysuser:x:999:999:"),
        "passwd should contain sysuser with uid/gid 999"
    );
}

#[test]
fn test_user_delete() {
    let passwd_file = create_temp_file("olduser:x:1001:1001::/home/olduser:/bin/bash\n");

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
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_path)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let passwd = std::fs::read_to_string(&passwd_path).expect("passwd file should exist");
    assert!(
        !passwd.contains("olduser"),
        "passwd should not contain olduser after deletion"
    );
}

#[test]
fn test_user_delete_nonexistent() {
    let passwd_file = create_temp_file("");

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module delete nonexistent user
  user:
    name: nonexistent
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_path)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok', got: {}",
        stdout
    );
}

#[test]
fn test_user_with_groups() {
    let passwd_file = create_temp_file("");

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
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_path)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let passwd = std::fs::read_to_string(&passwd_path).expect("passwd file should exist");
    assert!(
        passwd.contains("testuser:x:"),
        "passwd should contain testuser"
    );
}

#[test]
fn test_user_modify() {
    let passwd_file = create_temp_file("moduser:x:1002:1002:Old Comment:/home/moduser:/bin/sh\n");

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
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_path)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let passwd = std::fs::read_to_string(&passwd_path).expect("passwd file should exist");
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
}

#[test]
fn test_user_append_groups_already_present() {
    let passwd_file =
        create_temp_file("groupuser:x:1010:1010:Group User:/home/groupuser:/bin/bash\n");
    let group_file = create_temp_file(
        "docker:x:100:groupuser,otheruser\nwheel:x:101:groupuser\naudio:x:102:groupuser\nvideo:x:103:someoneelse\n",
    );

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module append groups already present
  user:
    name: groupuser
    state: present
    groups:
      - docker
      - wheel
    append: true
        "#
    .to_string();

    let args = ["--diff"];
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let group_path = group_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("RASH_TEST_PASSWD_FILE", &passwd_path),
            ("RASH_TEST_GROUP_FILE", &group_path),
        ],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        !stdout.contains("changed: TASK"),
        "stdout should NOT contain 'changed: TASK' (main task should be 'ok', not 'changed'), got: {}",
        stdout
    );
}

#[test]
fn test_user_append_groups_partially_present() {
    let passwd_file =
        create_temp_file("partialuser:x:1011:1011:Partial User:/home/partialuser:/bin/bash\n");
    let group_file = create_temp_file(
        "docker:x:100:partialuser\nwheel:x:101:otheruser\naudio:x:102:partialuser\n",
    );

    let script_text = r#"
#!/usr/bin/env rash
- name: test user module append groups partially present
  user:
    name: partialuser
    state: present
    groups:
      - docker
      - wheel
    append: true
        "#
    .to_string();

    let args = ["--diff"];
    let passwd_path = passwd_file.path().to_string_lossy().to_string();
    let group_path = group_file.path().to_string_lossy().to_string();
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("RASH_TEST_PASSWD_FILE", &passwd_path),
            ("RASH_TEST_GROUP_FILE", &group_path),
        ],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' when some groups need to be added, got: {}",
        stdout
    );
}
