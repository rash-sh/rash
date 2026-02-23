use crate::cli::modules::run_test;

#[test]
fn test_firewalld_service_enabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Allow HTTP traffic
  firewalld:
    service: http
    zone: public
    state: enabled
    permanent: true
    immediate: true
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("firewall-cmd")
            || stderr.contains("command not found")
    );
    assert!(stdout.contains("http") || !stderr.is_empty());
}

#[test]
fn test_firewalld_port_enabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Allow port 8080/tcp
  firewalld:
    port: 8080/tcp
    zone: public
    state: enabled
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("firewall-cmd")
            || stderr.contains("command not found")
    );
    assert!(stdout.contains("8080/tcp") || !stderr.is_empty());
}

#[test]
fn test_firewalld_service_disabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Block HTTPS traffic
  firewalld:
    service: https
    zone: public
    state: disabled
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("firewall-cmd")
            || stderr.contains("command not found")
    );
    assert!(stdout.contains("https") || !stderr.is_empty());
}

#[test]
fn test_firewalld_minimal() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Enable SSH
  firewalld:
    service: ssh
    state: enabled
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("firewall-cmd")
            || stderr.contains("command not found")
    );
    assert!(stdout.contains("ssh") || !stderr.is_empty());
}

#[test]
fn test_firewalld_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid firewalld call
  firewalld:
    service: http
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
fn test_firewalld_no_service_or_port() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Missing service and port
  firewalld:
    zone: public
    state: enabled
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("service") || stderr.contains("port"));
}

#[test]
fn test_firewalld_port_without_protocol() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Port without protocol
  firewalld:
    port: "8080"
    state: enabled
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("protocol"));
}
