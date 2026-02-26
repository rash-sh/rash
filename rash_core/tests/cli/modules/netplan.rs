use crate::cli::modules::run_test;

#[test]
fn test_netplan_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure netplan in check mode
  netplan:
    state: present
    renderer: networkd
    ethernets:
      eth0:
        dhcp4: true
    apply: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_netplan_dhcp_config() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure DHCP
  netplan:
    state: present
    renderer: networkd
    ethernets:
      eth0:
        dhcp4: true
    apply: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_netplan_static_ip_config() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure static IP
  netplan:
    state: present
    renderer: networkd
    ethernets:
      eth0:
        dhcp4: false
        addresses:
          - 192.168.1.100/24
        routes:
          - to: default
            via: 192.168.1.1
        nameservers:
          addresses:
            - 8.8.8.8
            - 8.8.4.4
    apply: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_netplan_bridge_config() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure bridge
  netplan:
    state: present
    renderer: networkd
    ethernets:
      eth0:
        dhcp4: false
    bridges:
      br0:
        interfaces:
          - eth0
        dhcp4: true
        parameters:
          stp: false
          forward-delay: 0
    apply: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_netplan_bond_config() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure bond
  netplan:
    state: present
    renderer: networkd
    ethernets:
      eth0:
        dhcp4: false
      eth1:
        dhcp4: false
    bonds:
      bond0:
        interfaces:
          - eth0
          - eth1
        addresses:
          - 192.168.1.100/24
        parameters:
          mode: 802.3ad
          lacp-rate: fast
    apply: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_netplan_with_config_param() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure using config parameter
  netplan:
    state: present
    config:
      network:
        version: 2
        renderer: networkd
        ethernets:
          eth0:
            dhcp4: true
    apply: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_netplan_networkmanager_renderer() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure with NetworkManager renderer
  netplan:
    state: present
    renderer: networkmanager
    ethernets:
      eth0:
        dhcp4: true
    apply: false
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_netplan_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure with invalid field
  netplan:
    state: present
    invalid_field: value
    ethernets:
      eth0:
        dhcp4: true
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown field"));
}
