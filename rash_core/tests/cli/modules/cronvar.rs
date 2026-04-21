use crate::cli::modules::run_test_with_env;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn get_unique_crontab_file() -> String {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_test_cronvar_{}", test_id)
}

#[test]
fn test_cronvar_add_variable() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test cronvar module add variable
  cronvar:
    name: PATH
    value: /usr/local/bin:/usr/bin:/bin
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_CRONTAB_FILE", &crontab_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&crontab_file);
}

#[test]
fn test_cronvar_update_variable() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    std::fs::write(&crontab_file, "PATH=/usr/bin:/bin\n").expect("Failed to create test crontab");

    let script_text = r#"
#!/usr/bin/env rash
- name: test cronvar module update variable
  cronvar:
    name: PATH
    value: /usr/local/bin:/usr/bin:/bin
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_CRONTAB_FILE", &crontab_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&crontab_file);
}

#[test]
fn test_cronvar_no_change_when_exists() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    std::fs::write(&crontab_file, "PATH=/usr/bin:/bin\n")
        .expect("Failed to create test crontab");

    let script_text = r#"
#!/usr/bin/env rash
- name: test cronvar module no change
  cronvar:
    name: PATH
    value: /usr/bin:/bin
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_CRONTAB_FILE", &crontab_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok' (no change), got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&crontab_file);
}

#[test]
fn test_cronvar_remove_variable() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    std::fs::write(&crontab_file, "MAILTO=root\nPATH=/usr/bin:/bin\n")
        .expect("Failed to create test crontab");

    let script_text = r#"
#!/usr/bin/env rash
- name: test cronvar module remove variable
  cronvar:
    name: MAILTO
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_CRONTAB_FILE", &crontab_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&crontab_file);
}
