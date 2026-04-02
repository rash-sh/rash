use std::fs;
use std::process::Command;

use crate::cli::modules::run_test;

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn docker_swarm_active() -> bool {
    let output = Command::new("docker")
        .args(["info", "--format", "{{.Swarm.LocalNodeState}}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    output.trim() == "active" || output.trim() == "locked"
}

fn init_swarm_if_needed() {
    let output = Command::new("docker")
        .args(["info", "--format", "{{.Swarm.LocalNodeState}}"])
        .output()
        .expect("Failed to get swarm info");
    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if state == "inactive" {
        let _ = Command::new("docker")
            .args(["swarm", "init", "--advertise-addr", "127.0.0.1"])
            .output();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

macro_rules! skip_without_docker_swarm {
    () => {
        if !docker_available() {
            eprintln!("Skipping test: Docker not available");
            return;
        }
        init_swarm_if_needed();
        if !docker_swarm_active() {
            eprintln!("Skipping test: Docker Swarm not active");
            return;
        }
    };
}

fn cleanup_secret(name: &str) {
    let _ = Command::new("docker").args(["secret", "rm", name]).output();
}

#[test]
fn test_docker_secret_create() {
    skip_without_docker_swarm!();

    let secret_name = "rash-test-secret-create";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret
  docker_secret:
    name: {}
    data: my_secret_value
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
            "inspect",
            "--format",
            "{{.Spec.Name}}",
            secret_name,
        ])
        .output()
        .expect("Failed to check secret");
    assert!(output.status.success(), "Secret should exist");

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_create_idempotent() {
    skip_without_docker_swarm!();

    let secret_name = "rash-test-secret-idempotent";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret
  docker_secret:
    name: {}
    data: my_secret_value
"#,
        secret_name
    );

    let args = ["--diff"];
    let (stdout1, stderr1) = run_test(&script_text, &args);
    assert!(stderr1.is_empty(), "stderr should be empty: {}", stderr1);
    assert!(
        stdout1.contains("changed"),
        "First run should show changed: {}",
        stdout1
    );

    let (stdout2, stderr2) = run_test(&script_text, &args);
    assert!(stderr2.is_empty(), "stderr should be empty: {}", stderr2);
    assert!(
        !stdout2.contains("changed"),
        "Second run should not show changed: {}",
        stdout2
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_create_from_file() {
    skip_without_docker_swarm!();

    let secret_name = "rash-test-secret-from-file";
    cleanup_secret(secret_name);

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let secret_content = "my_file_secret_data";
    fs::write(temp_dir.path().join("secret.txt"), secret_content)
        .expect("Failed to write secret file");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret from file
  docker_secret:
    name: {}
    data_src: {}
"#,
        secret_name,
        temp_dir.path().join("secret.txt").to_str().unwrap()
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
            "inspect",
            "--format",
            "{{.Spec.Name}}",
            secret_name,
        ])
        .output()
        .expect("Failed to check secret");
    assert!(output.status.success(), "Secret should exist");

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_create_with_labels() {
    skip_without_docker_swarm!();

    let secret_name = "rash-test-secret-labels";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret with labels
  docker_secret:
    name: {}
    data: my_secret_data
    labels:
      environment: production
      service: api
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
            "inspect",
            "--format",
            "{{json .Spec.Labels}}",
            secret_name,
        ])
        .output()
        .expect("Failed to check secret labels");
    let labels_json = String::from_utf8_lossy(&output.stdout);
    assert!(
        labels_json.contains("environment") || labels_json.contains("production"),
        "Labels should be present: {}",
        labels_json
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_remove() {
    skip_without_docker_swarm!();

    let secret_name = "rash-test-secret-remove";

    let mut child = Command::new("docker")
        .args(["secret", "create", secret_name])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to create secret");

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(b"test_data").ok();
    }
    child.wait_with_output().ok();

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
        .args(["secret", "inspect", secret_name])
        .output();
    assert!(
        output.is_err() || !output.unwrap().status.success(),
        "Secret should not exist"
    );
}

#[test]
fn test_docker_secret_remove_absent() {
    skip_without_docker_swarm!();

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
fn test_docker_secret_force_update() {
    skip_without_docker_swarm!();

    let secret_name = "rash-test-secret-force";
    cleanup_secret(secret_name);

    let script_text1 = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret
  docker_secret:
    name: {}
    data: initial_value
"#,
        secret_name
    );

    let args = ["--diff"];
    let (stdout1, stderr1) = run_test(&script_text1, &args);
    assert!(stderr1.is_empty(), "stderr should be empty: {}", stderr1);
    assert!(
        stdout1.contains("changed"),
        "First run should show changed: {}",
        stdout1
    );

    let script_text2 = format!(
        r#"
#!/usr/bin/env rash
- name: Update secret with force
  docker_secret:
    name: {}
    data: new_value
    force: true
"#,
        secret_name
    );

    let (stdout2, stderr2) = run_test(&script_text2, &args);
    assert!(stderr2.is_empty(), "stderr should be empty: {}", stderr2);
    assert!(
        stdout2.contains("changed"),
        "Force update should show changed: {}",
        stdout2
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_check_mode() {
    skip_without_docker_swarm!();

    let secret_name = "rash-test-secret-check";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create secret
  docker_secret:
    name: {}
    data: my_secret_data
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
        .args(["secret", "inspect", secret_name])
        .output();
    assert!(
        output.is_err() || !output.unwrap().status.success(),
        "Secret should NOT be created in check mode"
    );
}
