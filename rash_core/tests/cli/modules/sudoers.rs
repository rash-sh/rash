use crate::cli::modules::run_test;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_sudoers_check_mode() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Allow nginx to restart service
  sudoers:
    name: nginx-service
    user: nginx
    commands: /usr/sbin/service nginx restart
    nopassword: true
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
    assert!(stdout.contains("NOPASSWD"));
}

#[test]
fn test_sudoers_add_rule() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Allow nginx to restart service
  sudoers:
    name: nginx-service
    user: nginx
    commands: /usr/sbin/service nginx restart
    nopassword: true
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let rule_path = sudoers_path.join("nginx-service");
    assert!(rule_path.exists());

    let content = fs::read_to_string(&rule_path).unwrap();
    assert!(content.contains("nginx"));
    assert!(content.contains("NOPASSWD"));
}

#[test]
fn test_sudoers_no_change() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");
    fs::create_dir_all(&sudoers_path).unwrap();

    let rule_path = sudoers_path.join("nginx-service");
    fs::write(
        &rule_path,
        "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Allow nginx to restart service
  sudoers:
    name: nginx-service
    user: nginx
    commands: /usr/sbin/service nginx restart
    nopassword: true
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(!stdout.contains("changed:"));
}

#[test]
fn test_sudoers_remove_rule() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");
    fs::create_dir_all(&sudoers_path).unwrap();

    let rule_path = sudoers_path.join("nginx-service");
    fs::write(
        &rule_path,
        "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove nginx sudoers rule
  sudoers:
    name: nginx-service
    user: nginx
    commands: /usr/sbin/service nginx restart
    state: absent
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
    assert!(!rule_path.exists());
}

#[test]
fn test_sudoers_multiple_commands() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Allow nginx multiple commands
  sudoers:
    name: nginx-service
    user: nginx
    commands:
      - /usr/sbin/service nginx restart
      - /usr/sbin/service nginx status
    nopassword: true
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let rule_path = sudoers_path.join("nginx-service");
    let content = fs::read_to_string(&rule_path).unwrap();
    assert!(content.contains("/usr/sbin/service nginx restart"));
    assert!(content.contains("/usr/sbin/service nginx status"));
}

#[test]
fn test_sudoers_group_user() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Allow developers group docker access
  sudoers:
    name: docker-developers
    user: "%developers"
    commands: /usr/bin/docker
    nopassword: true
    setenv: true
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let rule_path = sudoers_path.join("docker-developers");
    let content = fs::read_to_string(&rule_path).unwrap();
    assert!(content.contains("%developers"));
    assert!(content.contains("NOPASSWD"));
    assert!(content.contains("SETENV"));
}

#[test]
fn test_sudoers_invalid_name_dot() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Invalid sudoers name
  sudoers:
    name: nginx.service
    user: nginx
    commands: /usr/sbin/service nginx restart
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("periods"));
}

#[test]
fn test_sudoers_all_commands() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Allow admin all commands
  sudoers:
    name: admin-user
    user: admin
    commands: ALL
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));

    let rule_path = sudoers_path.join("admin-user");
    let content = fs::read_to_string(&rule_path).unwrap();
    assert!(content.contains("admin ALL=(ALL) ALL"));
}

#[test]
fn test_sudoers_diff_output() {
    let dir = tempdir().unwrap();
    let sudoers_path = dir.path().join("sudoers.d");
    fs::create_dir_all(&sudoers_path).unwrap();

    let rule_path = sudoers_path.join("nginx-service");
    fs::write(
        &rule_path,
        "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx status\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Modify nginx sudoers rule
  sudoers:
    name: nginx-service
    user: nginx
    commands: /usr/sbin/service nginx restart
    nopassword: true
    sudoers_path: {}
        "#,
        sudoers_path.display()
    );

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("-") || stdout.contains("+"));
}
