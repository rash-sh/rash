use crate::cli::modules::run_test_with_env;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn get_unique_logrotate_file() -> String {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_test_logrotate_{}", test_id)
}

#[test]
fn test_logrotate_create_config() {
    let logrotate_file = get_unique_logrotate_file();
    let _ = std::fs::remove_file(&logrotate_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test logrotate module create config
  logrotate:
    path: /var/log/app.log
    frequency: daily
    rotate: 7
    compress: true
    missingok: true
    notifempty: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_LOGROTATE_FILE", &logrotate_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&logrotate_file);
}

#[test]
fn test_logrotate_multiple_paths() {
    let logrotate_file = get_unique_logrotate_file();
    let _ = std::fs::remove_file(&logrotate_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test logrotate module multiple paths
  logrotate:
    path:
      - /var/log/app1.log
      - /var/log/app2.log
    frequency: weekly
    rotate: 4
    compress: true
    delaycompress: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_LOGROTATE_FILE", &logrotate_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&logrotate_file);
}

#[test]
fn test_logrotate_with_size() {
    let logrotate_file = get_unique_logrotate_file();
    let _ = std::fs::remove_file(&logrotate_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test logrotate module with size
  logrotate:
    path: /var/log/large-app.log
    size: 100M
    rotate: 5
    compress: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_LOGROTATE_FILE", &logrotate_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&logrotate_file);
}

#[test]
fn test_logrotate_remove_config() {
    let logrotate_file = get_unique_logrotate_file();
    let _ = std::fs::remove_file(&logrotate_file);

    std::fs::write(
        &logrotate_file,
        "/var/log/app.log {\n  daily\n  rotate 7\n}\n",
    )
    .expect("Failed to create test logrotate file");

    let script_text = r#"
#!/usr/bin/env rash
- name: test logrotate module remove config
  logrotate:
    path: /var/log/app.log
    state: absent
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_LOGROTATE_FILE", &logrotate_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&logrotate_file);
}

#[test]
fn test_logrotate_no_change_when_exists() {
    let logrotate_file = get_unique_logrotate_file();
    let _ = std::fs::remove_file(&logrotate_file);

    std::fs::write(
        &logrotate_file,
        "/var/log/app.log\n{\n  daily\n  rotate 7\n  compress\n  missingok\n  notifempty\n}\n",
    )
    .expect("Failed to create test logrotate file");

    let script_text = r#"
#!/usr/bin/env rash
- name: test logrotate module no change
  logrotate:
    path: /var/log/app.log
    frequency: daily
    rotate: 7
    compress: true
    missingok: true
    notifempty: true
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_LOGROTATE_FILE", &logrotate_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok' (no change), got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&logrotate_file);
}

#[test]
fn test_logrotate_with_create() {
    let logrotate_file = get_unique_logrotate_file();
    let _ = std::fs::remove_file(&logrotate_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test logrotate module with create
  logrotate:
    path: /var/log/app.log
    frequency: daily
    rotate: 7
    create: "0644 root root"
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_LOGROTATE_FILE", &logrotate_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&logrotate_file);
}
