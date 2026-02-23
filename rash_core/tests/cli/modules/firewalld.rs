use crate::cli::modules::run_test;

fn firewalld_available() -> bool {
    std::process::Command::new("firewall-cmd")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_firewalld_parse_params_service() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld module parameters
  firewalld:
    service: http
    zone: public
    state: enabled
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("\"zone\":\"public\""));
    assert!(stdout.contains("\"state\":\"enabled\""));
}

#[test]
fn test_firewalld_parse_params_port() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld port parameter
  firewalld:
    port: "8080/tcp"
    zone: trusted
    state: disabled
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("\"zone\":\"trusted\""));
    assert!(stdout.contains("\"state\":\"disabled\""));
}

#[test]
fn test_firewalld_parse_params_interface() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld interface parameter
  firewalld:
    interface: eth0
    zone: internal
    state: enabled
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_firewalld_parse_params_masquerade() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld masquerade parameter
  firewalld:
    masquerade: true
    zone: public
    state: enabled
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_firewalld_parse_params_source() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld source parameter
  firewalld:
    source: 192.168.1.0/24
    zone: trusted
    state: present
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("\"state\":\"present\""));
}

#[test]
fn test_firewalld_parse_params_rich_rule() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld rich rule parameter
  firewalld:
    rich_rule: 'rule service name="ftp" accept'
    zone: public
    state: enabled
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_firewalld_permanent_option() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld permanent option
  firewalld:
    service: https
    zone: public
    state: enabled
    permanent: true
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("\"permanent\":true"));
}

#[test]
fn test_firewalld_invalid_port_format() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld with invalid port format
  firewalld:
    port: "8080"
    zone: public
    state: enabled
    "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Port must include protocol"));
}

#[test]
fn test_firewalld_invalid_zone_empty() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld with empty zone
  firewalld:
    service: http
    zone: ""
    state: enabled
    "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Zone cannot be empty"));
}

#[test]
fn test_firewalld_invalid_port_format_no_protocol() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld with port missing protocol
  firewalld:
    port: "53"
    zone: public
    state: enabled
    "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Port must include protocol"));
}

#[test]
fn test_firewalld_invalid_protocol() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld with invalid protocol
  firewalld:
    port: "8080/invalid"
    zone: public
    state: enabled
    "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Invalid protocol"));
}

#[test]
fn test_firewalld_default_zone() {
    if !firewalld_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: test firewalld default zone
  firewalld:
    service: http
    state: enabled
  register: result
- debug:
    msg: "{{ result.extra }}"
    "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("\"zone\":\"public\""));
}
