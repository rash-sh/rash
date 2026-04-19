use crate::cli::modules::run_test;

fn restic_available() -> bool {
    std::process::Command::new("restic")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_restic_parse_params() {
    if !restic_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Test restic params parsing
  restic:
    repository: /tmp/test-repo
    password: testpassword
    state: init
  check_mode: true
"#;

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
}

#[test]
fn test_restic_invalid_state_backup_no_path() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Backup without path
  restic:
    repository: /tmp/test-repo
    password: testpassword
    state: backup
"#;

    let args = ["--diff"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(
        stderr.contains("requires 'path'"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn test_restic_invalid_state_restore_no_restore_path() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Restore without restore_path
  restic:
    repository: /tmp/test-repo
    password: testpassword
    state: restore
"#;

    let args = ["--diff"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(
        stderr.contains("requires 'restore_path'"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn test_restic_invalid_state_forget_no_retention() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Forget without retention
  restic:
    repository: /tmp/test-repo
    password: testpassword
    state: forget
"#;

    let args = ["--diff"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(
        stderr.contains("retention policy"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn test_restic_register_result() {
    if !restic_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Init repo and register
  restic:
    repository: /tmp/test-repo
    password: testpassword
    state: init
  register: result
- debug:
    msg: "{{ result.extra }}"
"#;

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("state"),
        "stdout should contain 'state': {}",
        stdout
    );
}
