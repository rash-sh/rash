use crate::cli::modules::run_test;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_pam_limits_check_mode() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");
    fs::write(&limits_file, "# Default limits\n* soft nofile 1024\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set max open files
  pam_limits:
    domain: nginx
    limit_type: soft
    item: nofile
    value: "65535"
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
    assert!(stdout.contains("65535"));
}

#[test]
fn test_pam_limits_add_entry() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");
    fs::write(&limits_file, "* soft nofile 1024\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add nginx limits
  pam_limits:
    domain: nginx
    limit_type: soft
    item: nofile
    value: "65535"
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let content = fs::read_to_string(&limits_file).unwrap();
    assert!(content.contains("nginx"));
    assert!(content.contains("65535"));
}

#[test]
fn test_pam_limits_no_change() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");
    fs::write(&limits_file, "nginx\tsoft\tnofile\t65535\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set max open files
  pam_limits:
    domain: nginx
    limit_type: soft
    item: nofile
    value: "65535"
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(!stdout.contains("changed:"));
}

#[test]
fn test_pam_limits_remove_entry() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");
    fs::write(&limits_file, "nginx soft nofile 65535\n* hard nproc 4096\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove nginx limits
  pam_limits:
    domain: nginx
    limit_type: soft
    item: nofile
    state: absent
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let content = fs::read_to_string(&limits_file).unwrap();
    assert!(!content.contains("nginx"));
    assert!(content.contains("* hard nproc 4096"));
}

#[test]
fn test_pam_limits_with_comment() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set limits with comment
  pam_limits:
    domain: nginx
    limit_type: soft
    item: nofile
    value: "65535"
    dest: {}
    comment: High file descriptor limit
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let content = fs::read_to_string(&limits_file).unwrap();
    assert!(content.contains("# High file descriptor limit"));
}

#[test]
fn test_pam_limits_wildcard_domain() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set wildcard limits
  pam_limits:
    domain: '*'
    limit_type: hard
    item: nproc
    value: "4096"
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let content = fs::read_to_string(&limits_file).unwrap();
    assert!(content.contains("*"));
    assert!(content.contains("hard"));
    assert!(content.contains("nproc"));
    assert!(content.contains("4096"));
}

#[test]
fn test_pam_limits_group_domain() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set group limits
  pam_limits:
    domain: "@developers"
    limit_type: "-"
    item: nofile
    value: "100000"
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let content = fs::read_to_string(&limits_file).unwrap();
    assert!(content.contains("@developers"));
}

#[test]
fn test_pam_limits_unlimited_value() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set unlimited memlock
  pam_limits:
    domain: nginx
    limit_type: soft
    item: memlock
    value: unlimited
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let content = fs::read_to_string(&limits_file).unwrap();
    assert!(content.contains("unlimited"));
}

#[test]
fn test_pam_limits_missing_value_for_present() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set limits without value
  pam_limits:
    domain: nginx
    limit_type: soft
    item: nofile
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("value parameter is required"));
}

#[test]
fn test_pam_limits_diff_output() {
    let dir = tempdir().unwrap();
    let limits_file = dir.path().join("limits.conf");
    fs::write(&limits_file, "nginx soft nofile 1024\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Modify limits
  pam_limits:
    domain: nginx
    limit_type: soft
    item: nofile
    value: "65535"
    dest: {}
        "#,
        limits_file.display()
    );

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("-") || stdout.contains("+"));
    assert!(stdout.contains("65535"));
}
