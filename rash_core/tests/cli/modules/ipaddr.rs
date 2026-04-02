use crate::cli::modules::run_test;

#[test]
fn test_ipaddr_parse_params() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IP address
  ipaddr:
    interface: eth0
    address: 192.168.1.10/24
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_ipaddr_invalid_empty_interface() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IP address with empty interface
  ipaddr:
    interface: ""
    address: 192.168.1.10/24
        "#
    .to_string();

    let args = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Interface cannot be empty"));
}

#[test]
fn test_ipaddr_invalid_empty_address() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add empty IP address
  ipaddr:
    interface: eth0
    address: ""
        "#
    .to_string();

    let args = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Address cannot be empty"));
}

#[test]
fn test_ipaddr_invalid_address_no_cidr() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IP address without CIDR
  ipaddr:
    interface: eth0
    address: 192.168.1.10
        "#
    .to_string();

    let args = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("CIDR notation"));
}

#[test]
fn test_ipaddr_invalid_cidr_ipv4() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IP address with invalid CIDR
  ipaddr:
    interface: eth0
    address: 192.168.1.10/33
        "#
    .to_string();

    let args = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("IPv4 CIDR must be between 0 and 32"));
}

#[test]
fn test_ipaddr_invalid_cidr_ipv6() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IPv6 address with invalid CIDR
  ipaddr:
    interface: eth0
    address: 2001:db8::1/129
        "#
    .to_string();

    let args = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("IPv6 CIDR must be between 0 and 128"));
}

#[test]
fn test_ipaddr_invalid_cidr_format() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IP address with invalid CIDR format
  ipaddr:
    interface: eth0
    address: 192.168.1.10/abc
        "#
    .to_string();

    let args = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Invalid CIDR notation"));
}

#[test]
fn test_ipaddr_ipv6_valid() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IPv6 address
  ipaddr:
    interface: eth0
    address: 2001:db8::1/64
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_ipaddr_state_absent() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Remove IP address
  ipaddr:
    interface: eth0
    address: 192.168.1.10/24
    state: absent
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_ipaddr_ipv6_explicit_family() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IPv6 address with explicit family
  ipaddr:
    interface: eth0
    address: 2001:db8::1/64
    family: ipv6
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_ipaddr_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Add IP address with invalid field
  ipaddr:
    interface: eth0
    address: 192.168.1.10/24
    invalid: value
        "#
    .to_string();

    let args = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}
