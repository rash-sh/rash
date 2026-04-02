use crate::cli::modules::run_test;

#[test]
fn test_wakeonlan_basic() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Send Wake-on-LAN packet
  wakeonlan:
    mac: "00:11:22:33:44:55"
  register: result

- name: Verify result
  debug:
    var: result
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Wake-on-LAN packet sent to 00:11:22:33:44:55"));
}

#[test]
fn test_wakeonlan_with_custom_broadcast() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Send Wake-on-LAN packet with custom broadcast
  wakeonlan:
    mac: "AA:BB:CC:DD:EE:FF"
    broadcast: "192.168.1.255"
    port: 7
  register: result

- name: Verify result
  debug:
    var: result
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Wake-on-LAN packet sent to AA:BB:CC:DD:EE:FF"));
}

#[test]
fn test_wakeonlan_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test wakeonlan in check mode
  wakeonlan:
    mac: "00:11:22:33:44:55"
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Would send Wake-on-LAN packet"));
}

#[test]
fn test_wakeonlan_invalid_mac() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Send Wake-on-LAN with invalid MAC
  wakeonlan:
    mac: "invalid_mac"
        "#
    .to_string();

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.contains("Invalid MAC address"));
}
