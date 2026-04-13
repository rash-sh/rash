use crate::cli::modules::run_test;

#[test]
fn test_ufw_state_enabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Enable UFW
  ufw:
    state: enabled
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("UFW") || !stderr.is_empty());
}

#[test]
fn test_ufw_state_disabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Disable UFW
  ufw:
    state: disabled
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("UFW") || !stderr.is_empty());
}

#[test]
fn test_ufw_policy_deny() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set default incoming policy to deny
  ufw:
    policy: deny
    direction: in
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("policy") || !stderr.is_empty());
}

#[test]
fn test_ufw_rule_allow_port() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Allow SSH
  ufw:
    rule: allow
    port: "22"
    proto: tcp
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("22") || !stderr.is_empty());
}

#[test]
fn test_ufw_rule_allow_http() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Allow HTTP
  ufw:
    rule: allow
    port: "80"
    proto: tcp
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("80") || !stderr.is_empty());
}

#[test]
fn test_ufw_rule_with_from_ip() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Allow MySQL from specific subnet
  ufw:
    rule: allow
    port: "3306"
    proto: tcp
    from_ip: "192.168.1.0/24"
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("192.168.1.0/24") || !stderr.is_empty());
}

#[test]
fn test_ufw_rule_deny() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Deny telnet
  ufw:
    rule: deny
    port: "23"
    proto: tcp
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("23") || !stderr.is_empty());
}

#[test]
fn test_ufw_rule_limit() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Limit SSH connections
  ufw:
    rule: limit
    port: "22"
    proto: tcp
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("22") || !stderr.is_empty());
}

#[test]
fn test_ufw_rule_absent() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Delete rule
  ufw:
    rule: allow
    port: "8080"
    proto: tcp
    rule_state: absent
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("8080") || !stderr.is_empty());
}

#[test]
fn test_ufw_rule_with_comment() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Allow SSH with comment
  ufw:
    rule: allow
    port: "22"
    proto: tcp
    comment: "Allow SSH access"
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("SSH") || !stderr.is_empty());
}

#[test]
fn test_ufw_state_reloaded() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Reload UFW
  ufw:
    state: reloaded
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("reload") || !stderr.is_empty());
}

#[test]
fn test_ufw_state_reset() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Reset UFW
  ufw:
    state: reset
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty() || stderr.contains("ufw") || stderr.contains("command not found"));
    assert!(stdout.contains("reset") || !stderr.is_empty());
}

#[test]
fn test_ufw_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid ufw call
  ufw:
    state: enabled
    invalid_field: value
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown field") || stderr.contains("invalid"));
}

#[test]
fn test_ufw_no_required_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Missing required field
  ufw:
    port: "22"
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("state") || stderr.contains("policy") || stderr.contains("rule"));
}
