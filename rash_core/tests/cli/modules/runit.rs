use crate::cli::modules::run_test;

#[test]
fn test_runit_start_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module start service
  runit:
    name: nginx
    state: started
    enabled: true
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed"));
}

#[test]
fn test_runit_stop_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module stop service
  runit:
    name: nginx
    state: stopped
    enabled: false
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_runit_restart_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module restart service
  runit:
    name: nginx
    state: restarted
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed"));
}

#[test]
fn test_runit_reload_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module reload service
  runit:
    name: nginx
    state: reloaded
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed"));
}

#[test]
fn test_runit_enable_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module enable service
  runit:
    name: nginx
    enabled: true
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed"));
}

#[test]
fn test_runit_disable_service() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module disable service
  runit:
    name: nginx
    enabled: false
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.ends_with("ok\n"));
}

#[test]
fn test_runit_with_service_dir() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module with custom service directory
  runit:
    name: nginx
    state: started
    service_dir: /etc/runit/sv
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed"));
}

#[test]
fn test_runit_result_extra() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module result extra
  runit:
    name: nginx
    state: started
    enabled: true
  register: service_status
- debug:
    msg: "{{ service_status.extra }}"
        "#
    .to_string();

    let args = ["--output", "raw", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let last_line = stdout.lines().last().unwrap().replace(' ', "");
    assert!(last_line.contains("\"name\":\"nginx\""));
    assert!(last_line.contains("\"enabled\":true"));
}

#[test]
fn test_runit_both_state_and_enabled() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test runit module with both state and enabled
  runit:
    name: nginx
    state: started
    enabled: true
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed"));
}
