use crate::cli::modules::run_test_with_env;

#[test]
fn test_at_schedule_job() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test at module schedule job
  at:
    command: /usr/local/bin/backup.sh
    at_time: "now + 1 hour"
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(&script_text, &args, &[("RASH_TEST_AT_JOBS", "")]);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );
}

#[test]
fn test_at_schedule_unique_job() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test at module schedule unique job
  at:
    command: /usr/local/bin/maintenance.sh
    at_time: "23:00"
    unique: true
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(&script_text, &args, &[("RASH_TEST_AT_JOBS", "")]);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );
}

#[test]
fn test_at_remove_job() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test at module remove job
  at:
    command: /usr/local/bin/old-task.sh
    state: absent
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(&script_text, &args, &[("RASH_TEST_AT_JOBS", "")]);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok' (no change), got: {}",
        stdout
    );
}

#[test]
fn test_at_no_change_when_exists() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test at module no change
  at:
    command: /usr/local/bin/backup.sh
    at_time: "now + 1 hour"
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[
            ("RASH_TEST_AT_JOBS", "1\tMon Feb 24 2026\t12:00\ta\troot"),
            ("RASH_TEST_AT_CMD_PREFIX", "/usr/local/bin/backup.sh"),
        ],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok' (no change), got: {}",
        stdout
    );
}

#[test]
fn test_at_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test at module check mode
  at:
    command: /usr/local/bin/check.sh
    at_time: "now"
    state: present
    "#
    .to_string();

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test_with_env(&script_text, &args, &[("RASH_TEST_AT_JOBS", "")]);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );
}
