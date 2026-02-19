use crate::cli::modules::run_test_with_env;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn get_unique_crontab_file() -> String {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_test_crontab_{}", test_id)
}

#[test]
fn test_cron_add_job() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test cron module add job
  cron:
    name: daily-backup
    minute: "0"
    hour: "2"
    job: /usr/local/bin/backup.sh
    state: present
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
fn test_cron_add_special_time() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    let script_text = r#"
#!/usr/bin/env rash
- name: test cron module add with special time
  cron:
    name: weekly-cleanup
    special_time: weekly
    job: /usr/local/bin/cleanup.sh
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
fn test_cron_remove_job() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    std::fs::write(
        &crontab_file,
        "# rash: old-job\n0 2 * * * /usr/local/bin/old.sh\n",
    )
    .expect("Failed to create test crontab file");

    let script_text = r#"
#!/usr/bin/env rash
- name: test cron module remove job
  cron:
    name: old-job
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

#[test]
fn test_cron_no_change_when_exists() {
    let crontab_file = get_unique_crontab_file();
    let _ = std::fs::remove_file(&crontab_file);

    std::fs::write(
        &crontab_file,
        "# rash: existing-job\n0 2 * * * /usr/local/bin/backup.sh\n",
    )
    .expect("Failed to create test crontab file");

    let script_text = r#"
#!/usr/bin/env rash
- name: test cron module no change
  cron:
    name: existing-job
    minute: "0"
    hour: "2"
    job: /usr/local/bin/backup.sh
    state: present
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
