use std::process::Command;

use crate::cli::modules::{docker_test_lock, run_test};

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn swarm_active() -> bool {
    if !docker_available() {
        return false;
    }
    let output = Command::new("docker")
        .args(["info", "--format", "{{.Swarm.LocalNodeState}}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    output == "active"
}

macro_rules! skip_without_swarm {
    () => {
        if !swarm_active() {
            eprintln!("Skipping test: Docker Swarm not active");
            return;
        }
        let _lock = docker_test_lock();
    };
}

fn cleanup_secret(name: &str) {
    let _ = Command::new("docker").args(["secret", "rm", name]).output();
}

#[test]
fn test_docker_secret_check_mode() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-check";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create secret
  docker_secret:
    name: {}
    data: "test_value"
"#,
        secret_name
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
            "secret",
            "ls",
            "--filter",
            &format!("name={}", secret_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check secret status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(secret_name),
        "Secret should NOT be created in check mode"
    );
}

#[test]
fn test_docker_secret_create() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-create";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create a secret
  docker_secret:
    name: {}
    data: "my_secret_password"
"#,
        secret_name
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
            "secret",
            "ls",
            "--filter",
            &format!("name={}", secret_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check secret status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains(secret_name),
        "Secret should be created"
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_create_idempotent() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-idempotent";
    cleanup_secret(secret_name);

    let _ = Command::new("docker")
        .args(["secret", "create", secret_name, "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"initial_value").unwrap();
            }
            child.wait_with_output()
        });

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create existing secret
  docker_secret:
    name: {}
    data: "my_secret_password"
"#,
        secret_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for existing secret: {}",
        stdout
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_create_with_labels() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-labels";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret with labels
  docker_secret:
    name: {}
    data: "secret_with_labels"
    labels:
      environment: production
      owner: team-ops
"#,
        secret_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_force_update() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-force";
    cleanup_secret(secret_name);

    let _ = Command::new("docker")
        .args(["secret", "create", secret_name, "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"old_value").unwrap();
            }
            child.wait_with_output()
        });

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Force update secret
  docker_secret:
    name: {}
    data: "new_value"
    force: true
"#,
        secret_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_remove() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-remove";
    cleanup_secret(secret_name);

    let _ = Command::new("docker")
        .args(["secret", "create", secret_name, "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"test_value").unwrap();
            }
            child.wait_with_output()
        });

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove secret
  docker_secret:
    name: {}
    state: absent
"#,
        secret_name
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
            "secret",
            "ls",
            "--filter",
            &format!("name={}", secret_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check secret status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(secret_name),
        "Secret should be removed"
    );
}

#[test]
fn test_docker_secret_remove_absent() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-absent";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove non-existent secret
  docker_secret:
    name: {}
    state: absent
"#,
        secret_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for absent secret: {}",
        stdout
    );
}

#[test]
fn test_docker_secret_create_from_file() {
    skip_without_swarm!();

    let secret_name = "rash-test-secret-file";
    cleanup_secret(secret_name);

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("password.txt");
    std::fs::write(&file_path, "file_secret_value").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret from file
  docker_secret:
    name: {}
    data_src: {}
"#,
        secret_name,
        file_path.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    cleanup_secret(secret_name);
}
