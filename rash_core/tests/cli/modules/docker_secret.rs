use std::process::Command;

use crate::cli::modules::run_test;

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
    Command::new("docker")
        .args(["info", "--format", "{{.Swarm.LocalNodeState}}"])
        .output()
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.trim() == "active"
        })
        .unwrap_or(false)
}

macro_rules! skip_without_swarm {
    () => {
        if !swarm_active() {
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
    skip_without_swarm!();

    let secret_name = "test-rash-secret";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret
  docker_secret:
    name: {}
    data: "my-secret-value"
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
    skip_without_swarm!();

    let secret_name = "test-rash-secret-idem";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret
  docker_secret:
    name: {}
    data: "my-secret-value"
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
fn test_docker_secret_create_with_labels() {
    skip_without_swarm!();

    let secret_name = "test-rash-secret-labels";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret with labels
  docker_secret:
    name: {}
    data: "secret-with-labels"
    labels:
      environment: test
      service: rash
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
    assert!(output.status.success(), "Secret should exist");

    let labels: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap_or_default();
    assert_eq!(labels.get("environment").unwrap().as_str().unwrap(), "test");
    assert_eq!(labels.get("service").unwrap().as_str().unwrap(), "rash");

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_remove() {
    skip_without_swarm!();

    let secret_name = "test-rash-secret-remove";

    let _ = Command::new("docker")
        .args(["secret", "create", secret_name])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(b"test-data").unwrap();
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
        .args(["secret", "inspect", secret_name])
        .output()
        .expect("Failed to check secret");
    assert!(!output.status.success(), "Secret should not exist");
}

#[test]
fn test_docker_secret_remove_idempotent() {
    skip_without_swarm!();

    let secret_name = "test-rash-secret-remove-idem";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove secret (not existing)
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
        "Removing non-existing secret should not show changed: {}",
        stdout
    );
}

#[test]
fn test_docker_secret_force_recreate() {
    skip_without_swarm!();

    let secret_name = "test-rash-secret-force";
    cleanup_secret(secret_name);

    let script_text1 = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret
  docker_secret:
    name: {}
    data: "initial-value"
"#,
        secret_name
    );

    let args = ["--diff"];
    let (stdout1, stderr1) = run_test(&script_text1, &args);
    assert!(stderr1.is_empty(), "stderr should be empty: {}", stderr1);
    assert!(stdout1.contains("changed"), "First run should show changed");

    let script_text2 = format!(
        r#"
#!/usr/bin/env rash
- name: Force recreate secret
  docker_secret:
    name: {}
    data: "new-value"
    force: true
"#,
        secret_name
    );

    let (stdout2, stderr2) = run_test(&script_text2, &args);
    assert!(stderr2.is_empty(), "stderr should be empty: {}", stderr2);
    assert!(
        stdout2.contains("changed"),
        "Force recreate should show changed: {}",
        stdout2
    );

    cleanup_secret(secret_name);
}

#[test]
fn test_docker_secret_check_mode() {
    skip_without_swarm!();

    let secret_name = "test-rash-secret-check";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create secret
  docker_secret:
    name: {}
    data: "test-value"
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
        .output()
        .expect("Failed to check secret");
    assert!(
        !output.status.success(),
        "Secret should NOT be created in check mode"
    );
}

#[test]
fn test_docker_secret_base64() {
    skip_without_swarm!();

    let secret_name = "test-rash-secret-b64";
    cleanup_secret(secret_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create secret from base64
  docker_secret:
    name: {}
    data: c2VjcmV0LWRhdGE=
    data_is_b64: true
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
fn test_docker_secret_invalid_name() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create secret with invalid name
  docker_secret:
    name: "invalid secret name"
    data: test
"#;

    let args = ["--output", "raw"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(
        stderr.contains("invalid characters") || stderr.contains("Error"),
        "stderr should contain error about invalid name: {}",
        stderr
    );
}

#[test]
fn test_docker_secret_missing_data() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create secret without data
  docker_secret:
    name: test-secret
    state: present
"#;

    let args = ["--output", "raw"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(
        stderr.contains("data is required") || stderr.contains("Error"),
        "stderr should contain error about missing data: {}",
        stderr
    );
}
