use crate::cli::modules::run_test;

#[test]
fn test_hostname_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set hostname in check mode
  hostname:
    name: check-mode-host
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
    assert!(stdout.contains("Set hostname to check-mode-host"));
}

#[test]
fn test_hostname_invalid_empty() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set empty hostname
  hostname:
    name: ""
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Hostname cannot be empty"));
}

#[test]
fn test_hostname_invalid_chars() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set hostname with invalid chars
  hostname:
    name: "invalid host"
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Invalid character"));
}

#[test]
fn test_hostname_invalid_starts_with_hyphen() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set hostname starting with hyphen
  hostname:
    name: "-invalid"
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("cannot start or end with hyphen"));
}

#[test]
fn test_hostname_invalid_ends_with_hyphen() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set hostname ending with hyphen
  hostname:
    name: "invalid-"
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("cannot start or end with hyphen"));
}

#[test]
fn test_hostname_valid_simple() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set simple hostname
  hostname:
    name: web01
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_hostname_valid_fqdn() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set FQDN hostname
  hostname:
    name: web01.example.com
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_hostname_valid_with_hyphen() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set hostname with hyphen
  hostname:
    name: my-host-01
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_hostname_invalid_too_long() {
    let long_name = "a".repeat(254);
    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set too long hostname
  hostname:
    name: "{}"
        "#,
        long_name
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Hostname too long"));
}

#[test]
fn test_hostname_diff_output() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set hostname
  hostname:
    name: new-hostname
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("-") || stdout.contains("+"));
    assert!(stdout.contains("new-hostname"));
}
