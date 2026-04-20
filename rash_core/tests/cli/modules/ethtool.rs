use crate::cli::modules::run_test;

#[test]
fn test_ethtool_query_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Query ethtool settings in check mode
  ethtool:
    device: eth0
    state: query
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (_stdout, _stderr) = run_test(&script_text, &args);
}

#[test]
fn test_ethtool_set_speed_duplex_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set speed and duplex in check mode
  ethtool:
    device: eth0
    speed: 1000
    duplex: full
    autoneg: true
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (_stdout, _stderr) = run_test(&script_text, &args);
}

#[test]
fn test_ethtool_invalid_device_empty() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set empty device
  ethtool:
    device: ""
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("device cannot be empty"));
}

#[test]
fn test_ethtool_invalid_device_too_long() {
    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set too long device name
  ethtool:
    device: "{}"
        "#,
        "a".repeat(16)
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("too long"));
}

#[test]
fn test_ethtool_invalid_device_chars() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set device with invalid chars
  ethtool:
    device: "eth 0"
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("invalid character"));
}

#[test]
fn test_ethtool_speed_without_duplex() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set speed without duplex
  ethtool:
    device: eth0
    speed: 1000
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("duplex is required"));
}

#[test]
fn test_ethtool_duplex_without_speed() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set duplex without speed
  ethtool:
    device: eth0
    duplex: full
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("speed is required"));
}

#[test]
fn test_ethtool_invalid_speed() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set invalid speed
  ethtool:
    device: eth0
    speed: 999
    duplex: full
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("invalid speed"));
}

#[test]
fn test_ethtool_offload_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure offload in check mode
  ethtool:
    device: eth0
    offload:
      rx: true
      tx: true
      tso: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (_stdout, _stderr) = run_test(&script_text, &args);
}

#[test]
fn test_ethtool_state_absent_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Reset device settings in check mode
  ethtool:
    device: eth0
    state: absent
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (_stdout, _stderr) = run_test(&script_text, &args);
}

#[test]
fn test_ethtool_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid field
  ethtool:
    device: eth0
    invalid_field: value
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}
