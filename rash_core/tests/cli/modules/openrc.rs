use crate::cli::modules::run_test;

#[test]
fn test_openrc_start_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module start service
  openrc:
    name: nginx
    state: started
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_openrc_stop_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module stop service
  openrc:
    name: nginx
    state: stopped
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_openrc_restart_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module restart service
  openrc:
    name: nginx
    state: restarted
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_openrc_enable_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module enable service
  openrc:
    name: nginx
    enabled: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_openrc_enable_service_boot_runlevel() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module enable service in boot runlevel
  openrc:
    name: nginx
    enabled: true
    runlevel: boot
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_openrc_disable_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module disable service
  openrc:
    name: nginx
    enabled: false
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_openrc_both_state_and_enabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module with both state and enabled
  openrc:
    name: nginx
    state: started
    enabled: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.ends_with("ok\n"));
}

#[test]
fn test_openrc_result_extra() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module result extra
  openrc:
    name: nginx
    state: started
    enabled: true
  register: service_status
- debug:
    msg: "{{ service_status.extra }}"
        "#
    .to_string();

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let last_line = stdout.lines().last().unwrap().replace(' ', "");
    assert!(last_line.contains("\"name\":\"nginx\""));
    assert!(last_line.contains("\"runlevel\":\"default\""));
}

#[test]
fn test_openrc_error_missing_name() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test openrc module error when missing name
  openrc:
    state: started
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}
