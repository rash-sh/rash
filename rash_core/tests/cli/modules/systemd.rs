use crate::cli::modules::run_test;

#[test]
fn test_systemd_start_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module start service
  systemd:
    name: nginx
    state: started
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed: Started nginx."));
}

#[test]
fn test_systemd_start_running_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module start already running service
  systemd:
    name: httpd
    state: started
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_systemd_stop_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module stop service
  systemd:
    name: httpd
    state: stopped
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("Stopped httpd."));
    assert!(stderr.is_empty());
    assert!(stdout.contains("changed: Stopped httpd."));
}

#[test]
fn test_systemd_stop_stopped_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module stop already stopped service
  systemd:
    name: nginx
    state: stopped
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("Stopped nginx."));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_systemd_restart_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module restart service
  systemd:
    name: httpd
    state: restarted
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("Restarted httpd."));
    assert!(stderr.is_empty());
    assert!(stdout.contains("changed: Restarted httpd."));
}

#[test]
fn test_systemd_reload_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module reload service
  systemd:
    name: httpd
    state: reloaded
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("Reloaded httpd."));
    assert!(stderr.is_empty());
    assert!(stdout.contains("changed: Reloaded httpd."));
}

#[test]
fn test_systemd_enable_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module enable service
  systemd:
    name: nginx
    enabled: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("Created symlink"));
    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") && stdout.contains("Created symlink"));
}

#[test]
fn test_systemd_enable_enabled_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module enable already enabled service
  systemd:
    name: sshd
    enabled: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("Created symlink"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_systemd_disable_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module disable service
  systemd:
    name: sshd
    enabled: false
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("Removed"));
    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") && stdout.contains("Removed"));
}

#[test]
fn test_systemd_disable_disabled_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module disable already disabled service
  systemd:
    name: nginx
    enabled: false
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(!stdout.contains("Removed"));
    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_systemd_daemon_reload() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module daemon reload
  systemd:
    daemon_reload: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_systemd_result_extra() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module result extra
  systemd:
    name: httpd
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
    // Check that the last line of the output contains JSON with active and enabled status
    let last_line = stdout.lines().last().unwrap().replace(' ', "");
    assert!(last_line.contains("\"name\":\"httpd\""));
    assert!(last_line.contains("\"active\":true"));
    assert!(last_line.contains("\"enabled\":true"));
}

#[test]
fn test_systemd_both_state_and_enabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module with both state and enabled
  systemd:
    name: nginx
    state: started
    enabled: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stdout.contains("Started nginx."));
    assert!(stdout.contains("Created symlink"));
    assert!(stderr.is_empty());
    assert!(
        stdout.contains("changed:")
            && stdout.contains("Started nginx.")
            && stdout.contains("Created symlink")
    );
}

#[test]
fn test_systemd_error_missing_name() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test systemd module error when missing name
  systemd:
    state: started
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Either name or daemon_reload is required"));
}
