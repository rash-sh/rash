use std::process::Command;

use crate::cli::modules::run_test;

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! skip_without_docker {
    () => {
        if !docker_available() {
            eprintln!("Skipping test: Docker not available");
            return;
        }
    };
}

fn cleanup_volume(name: &str) {
    let _ = Command::new("docker")
        .args(["volume", "rm", "-f", name])
        .output();
}

#[test]
fn test_docker_volume_check_mode() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-check";
    cleanup_volume(volume_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create volume
  docker_volume:
    name: {}
"#,
        volume_name
    );

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );

    let output = Command::new("docker")
        .args([
            "volume",
            "ls",
            "--filter",
            &format!("name={}", volume_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check volume status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(volume_name),
        "Volume should NOT be created in check mode"
    );
}

#[test]
fn test_docker_volume_create() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-create";
    cleanup_volume(volume_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create a volume
  docker_volume:
    name: {}
"#,
        volume_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args([
            "volume",
            "ls",
            "--filter",
            &format!("name={}", volume_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check volume status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains(volume_name),
        "Volume should be created"
    );

    cleanup_volume(volume_name);
}

#[test]
fn test_docker_volume_create_idempotent() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-idempotent";
    cleanup_volume(volume_name);

    let _ = Command::new("docker")
        .args(["volume", "create", volume_name])
        .output();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create existing volume
  docker_volume:
    name: {}
"#,
        volume_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for existing volume: {}",
        stdout
    );

    cleanup_volume(volume_name);
}

#[test]
fn test_docker_volume_create_with_driver() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-driver";
    cleanup_volume(volume_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create volume with driver
  docker_volume:
    name: {}
    driver: local
"#,
        volume_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args(["volume", "inspect", "--format", "{{.Driver}}", volume_name])
        .output()
        .expect("Failed to check volume driver");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains("local"),
        "Volume should use local driver"
    );

    cleanup_volume(volume_name);
}

#[test]
fn test_docker_volume_create_with_labels() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-labels";
    cleanup_volume(volume_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create volume with labels
  docker_volume:
    name: {}
    labels:
      environment: production
      owner: team-ops
"#,
        volume_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    cleanup_volume(volume_name);
}

#[test]
fn test_docker_volume_remove() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-remove";
    cleanup_volume(volume_name);

    let _ = Command::new("docker")
        .args(["volume", "create", volume_name])
        .output();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove volume
  docker_volume:
    name: {}
    state: absent
"#,
        volume_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args([
            "volume",
            "ls",
            "--filter",
            &format!("name={}", volume_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check volume status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(volume_name),
        "Volume should be removed"
    );
}

#[test]
fn test_docker_volume_remove_absent() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-absent";
    cleanup_volume(volume_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove non-existent volume
  docker_volume:
    name: {}
    state: absent
"#,
        volume_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for absent volume: {}",
        stdout
    );
}

#[test]
fn test_docker_volume_force_remove() {
    skip_without_docker!();

    let volume_name = "rash-test-volume-force";
    cleanup_volume(volume_name);

    let _ = Command::new("docker")
        .args(["volume", "create", volume_name])
        .output();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Force remove volume
  docker_volume:
    name: {}
    state: absent
    force: true
"#,
        volume_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args([
            "volume",
            "ls",
            "--filter",
            &format!("name={}", volume_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check volume status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(volume_name),
        "Volume should be removed"
    );
}
