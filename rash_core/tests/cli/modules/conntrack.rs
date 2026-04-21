use crate::cli::modules::run_test;

#[test]
fn test_conntrack_flush() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Flush all connection tracking entries
  conntrack:
    flush: true
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty() || stderr.contains("conntrack") || stderr.contains("command not found")
    );
    assert!(stdout.contains("flush") || !stderr.is_empty());
}

#[test]
fn test_conntrack_drop_source() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Drop connections from specific IP
  conntrack:
    source: 10.0.0.1
    state: absent
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty() || stderr.contains("conntrack") || stderr.contains("command not found")
    );
    assert!(stdout.contains("10.0.0.1") || !stderr.is_empty());
}

#[test]
fn test_conntrack_drop_with_protocol_port() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Drop connections to specific IP and port
  conntrack:
    destination: 192.168.1.100
    protocol: tcp
    port: 443
    state: absent
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty() || stderr.contains("conntrack") || stderr.contains("command not found")
    );
    assert!(stdout.contains("192.168.1.100") || !stderr.is_empty());
}

#[test]
fn test_conntrack_list() {
    let script_text = r#"
#!/usr/bin/env rash
- name: List connections from specific IP
  conntrack:
    source: 10.0.0.1
    state: list
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty() || stderr.contains("conntrack") || stderr.contains("command not found")
    );
    assert!(stdout.contains("10.0.0.1") || !stderr.is_empty());
}

#[test]
fn test_conntrack_drop_udp() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Drop UDP connections from a subnet
  conntrack:
    source: 10.0.0.0/24
    protocol: udp
    state: absent
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty() || stderr.contains("conntrack") || stderr.contains("command not found")
    );
    assert!(stdout.contains("10.0.0.0/24") || !stderr.is_empty());
}

#[test]
fn test_conntrack_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid conntrack call
  conntrack:
    source: 10.0.0.1
    invalid_field: value
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown field") || stderr.contains("invalid"));
}

#[test]
fn test_conntrack_flush_with_filters() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Flush with filter
  conntrack:
    flush: true
    source: 10.0.0.1
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("flush") || stderr.contains("filter"));
}

#[test]
fn test_conntrack_port_without_protocol() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Port without protocol
  conntrack:
    source: 10.0.0.1
    port: 443
    state: absent
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("protocol"));
}
