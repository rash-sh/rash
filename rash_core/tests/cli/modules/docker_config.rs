use crate::cli::modules::run_test;

use std::fs;
use tempfile::tempdir;

#[test]
fn test_docker_config_set_storage_driver() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set Docker storage driver
  docker_config:
    path: {}
    storage_driver: overlay2
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("overlay2"),
        "config should contain overlay2: {}",
        content
    );
}

#[test]
fn test_docker_config_check_mode() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not modify file
  docker_config:
    path: {}
    storage_driver: overlay2
"#,
        file_path.to_str().unwrap()
    );

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );

    assert!(
        !file_path.exists(),
        "file should NOT be created in check mode"
    );
}

#[test]
fn test_docker_config_idempotent() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    fs::write(&file_path, r#"{"storage-driver": "overlay2"}"#).unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set same storage driver (idempotent test)
  docker_config:
    path: {}
    storage_driver: overlay2
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for idempotent: {}",
        stdout
    );
}

#[test]
fn test_docker_config_multiple_options() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Configure multiple Docker options
  docker_config:
    path: {}
    storage_driver: overlay2
    log_driver: json-file
    live_restore: true
    debug: false
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("storage-driver"),
        "config should contain storage-driver"
    );
    assert!(
        content.contains("log-driver"),
        "config should contain log-driver"
    );
    assert!(
        content.contains("live-restore"),
        "config should contain live-restore"
    );
}

#[test]
fn test_docker_config_registry_mirrors() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Configure registry mirrors
  docker_config:
    path: {}
    registry_mirrors:
      - "https://mirror1.example.com"
      - "https://mirror2.example.com"
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("registry-mirrors"),
        "config should contain registry-mirrors"
    );
    assert!(
        content.contains("mirror1.example.com"),
        "config should contain mirror1"
    );
}

#[test]
fn test_docker_config_remove_option() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    fs::write(
        &file_path,
        r#"{"storage-driver": "overlay2", "debug": true}"#,
    )
    .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove debug option
  docker_config:
    path: {}
    debug: true
    state: absent
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        !content.contains("debug"),
        "config should not contain debug after removal"
    );
    assert!(
        content.contains("storage-driver"),
        "config should still contain storage-driver"
    );
}

#[test]
fn test_docker_config_arbitrary_key() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set arbitrary configuration key
  docker_config:
    path: {}
    key: custom.nested.option
    value: myvalue
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("custom"),
        "config should contain custom key"
    );
    assert!(
        content.contains("nested"),
        "config should contain nested key"
    );
    assert!(
        content.contains("option"),
        "config should contain option key"
    );
}

#[test]
fn test_docker_config_backup() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    fs::write(&file_path, r#"{"storage-driver": "devicemapper"}"#).unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Update storage driver with backup
  docker_config:
    path: {}
    storage_driver: overlay2
    backup: true
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let backups: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
        .collect();
    assert!(backups.len() == 1, "should have created one backup file");
}

#[test]
fn test_docker_config_log_opts() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("daemon.json");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Configure log options
  docker_config:
    path: {}
    log_driver: json-file
    log_opts:
      max-size: 10m
      max-file: 3
"#,
        file_path.to_str().unwrap()
    );

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(&script_text, args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("log-driver"),
        "config should contain log-driver"
    );
    assert!(
        content.contains("log-opts"),
        "config should contain log-opts"
    );
    assert!(
        content.contains("max-size"),
        "config should contain max-size"
    );
}
