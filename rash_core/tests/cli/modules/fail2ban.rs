use crate::cli::modules::run_test;

#[test]
fn test_fail2ban_create_jail() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create SSH jail
  fail2ban:
    name: sshd
    state: present
    enabled: true
    port: ssh
    filter: sshd
    logpath: /var/log/auth.log
    maxretry: 5
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("Permission denied")
            || stderr.contains("No such file")
    );
    assert!(stdout.contains("sshd") || !stderr.is_empty());
}

#[test]
fn test_fail2ban_create_nginx_jail() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create nginx jail
  fail2ban:
    name: nginx-http-auth
    state: present
    enabled: true
    port: http,https
    filter: nginx-http-auth
    logpath: /var/log/nginx/error.log
    maxretry: 3
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("Permission denied")
            || stderr.contains("No such file")
    );
    assert!(stdout.contains("nginx") || !stderr.is_empty());
}

#[test]
fn test_fail2ban_remove_jail() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Remove SSH jail
  fail2ban:
    name: sshd
    state: absent
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("Permission denied"));
    assert!(stdout.contains("sshd") || stdout.contains("does not exist") || !stderr.is_empty());
}

#[test]
fn test_fail2ban_disable_jail() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Disable SSH jail
  fail2ban:
    name: sshd
    enabled: false
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("Permission denied")
            || stderr.contains("No such file")
    );
    assert!(stdout.contains("sshd") || !stderr.is_empty());
}

#[test]
fn test_fail2ban_minimal() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create minimal jail
  fail2ban:
    name: my-jail
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("Permission denied")
            || stderr.contains("No such file")
    );
    assert!(stdout.contains("my-jail") || !stderr.is_empty());
}

#[test]
fn test_fail2ban_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid fail2ban call
  fail2ban:
    name: sshd
    invalid_field: value
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown field") || stderr.contains("invalid"));
}

#[test]
fn test_fail2ban_empty_name() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Empty jail name
  fail2ban:
    name: ""
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("empty") || stderr.contains("Jail name"));
}

#[test]
fn test_fail2ban_invalid_name() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid jail name
  fail2ban:
    name: "invalid/name"
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("alphanumeric") || stderr.contains("invalid"));
}
