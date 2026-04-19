use crate::cli::modules::run_test;

#[test]
fn test_syslog_with_minimal_params() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test syslog with minimal params
  syslog:
    msg: "Test message from rash"
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("Message logged to syslog"));
}

#[test]
fn test_syslog_with_all_params() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test syslog with all params
  syslog:
    msg: "Critical error in service"
    facility: daemon
    priority: error
    ident: testapp
    pid: true
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("Message logged to syslog"));
}

#[test]
fn test_syslog_with_local_facility() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test syslog with local0 facility
  syslog:
    msg: "Application log message"
    facility: local0
    priority: info
    ident: myapp
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("Message logged to syslog"));
}

#[test]
fn test_syslog_with_debug_priority() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test syslog with debug priority
  syslog:
    msg: "Debug information"
    priority: debug
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("Message logged to syslog"));
}

#[test]
fn test_syslog_missing_msg() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test syslog without required msg
  syslog:
    facility: daemon
        "#
    .to_string();

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_syslog_with_templated_msg() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set a variable
  set_vars:
    app_name: myapp

- name: Test syslog with templated message
  syslog:
    msg: "Application {{ app_name }} started"
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("Message logged to syslog"));
}
