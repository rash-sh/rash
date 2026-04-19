use std::process::Command;

use crate::cli::modules::run_test;

fn incus_available() -> bool {
    Command::new("incus")
        .args(["version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! skip_without_incus {
    () => {
        if !incus_available() {
            eprintln!("Skipping test: Incus not available");
            return;
        }
    };
}

fn cleanup_instance(name: &str) {
    let _ = Command::new("incus")
        .args(["delete", "--force", name])
        .output();
}

fn create_running_instance(name: &str) {
    let _ = Command::new("incus")
        .args(["init", "images:alpine/3.19", name])
        .output();

    let _ = Command::new("incus").args(["start", name]).output();

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let output = Command::new("incus")
        .args(["list", name, "--format", "json"])
        .output()
        .expect("Failed to check instance status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(instances) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout)
        && (instances.is_empty()
            || instances[0].get("status").and_then(|s| s.as_str()) != Some("Running"))
    {
        eprintln!("Warning: Instance {} is not running after creation", name);
    }
}

#[test]
fn test_incus_container_check_mode() {
    skip_without_incus!();

    let instance_name = "rash-test-container-check";
    cleanup_instance(instance_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create instance
  incus:
    name: {}
    image: images:alpine/3.19
    state: started
"#,
        instance_name
    );

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );

    let output = Command::new("incus")
        .args(["list", instance_name, "--format", "json"])
        .output()
        .expect("Failed to check instance status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    if let Ok(instances) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout_check) {
        assert!(
            instances.is_empty(),
            "Instance should NOT be created in check mode"
        );
    }
}

#[test]
fn test_incus_container_stop() {
    skip_without_incus!();

    let instance_name = "rash-test-container-stop";
    cleanup_instance(instance_name);
    create_running_instance(instance_name);

    let stop_script = format!(
        r#"
#!/usr/bin/env rash
- name: Stop container
  incus:
    name: {}
    state: stopped
"#,
        instance_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&stop_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("incus")
        .args(["list", instance_name, "--format", "json"])
        .output()
        .expect("Failed to check instance status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    if let Ok(instances) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout_check)
        && let Some(status) = instances
            .first()
            .and_then(|i| i.get("status").and_then(|s| s.as_str()))
    {
        assert!(status != "Running", "Instance should not be running");
    }

    cleanup_instance(instance_name);
}

#[test]
fn test_incus_container_remove() {
    skip_without_incus!();

    let instance_name = "rash-test-container-remove";
    cleanup_instance(instance_name);
    create_running_instance(instance_name);

    let stop_script = format!(
        r#"
#!/usr/bin/env rash
- name: Stop instance first
  incus:
    name: {}
    state: stopped
"#,
        instance_name
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&stop_script, &args);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after stop: {}",
        stderr
    );

    let remove_script = format!(
        r#"
#!/usr/bin/env rash
- name: Remove instance
  incus:
    name: {}
    state: absent
"#,
        instance_name
    );

    let (stdout, stderr) = run_test(&remove_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("incus")
        .args(["list", instance_name, "--format", "json"])
        .output()
        .expect("Failed to check instance status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    if let Ok(instances) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout_check) {
        assert!(instances.is_empty(), "Instance should be removed");
    }
}

#[test]
fn test_incus_container_restart() {
    skip_without_incus!();

    let instance_name = "rash-test-container-restart";
    cleanup_instance(instance_name);
    create_running_instance(instance_name);

    let restart_script = format!(
        r#"
#!/usr/bin/env rash
- name: Restart instance
  incus:
    name: {}
    state: restarted
"#,
        instance_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&restart_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    cleanup_instance(instance_name);
}

#[test]
fn test_incus_container_force_remove() {
    skip_without_incus!();

    let instance_name = "rash-test-container-force";
    cleanup_instance(instance_name);
    create_running_instance(instance_name);

    let remove_script = format!(
        r#"
#!/usr/bin/env rash
- name: Force remove running instance
  incus:
    name: {}
    state: absent
    force: true
"#,
        instance_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&remove_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("incus")
        .args(["list", instance_name, "--format", "json"])
        .output()
        .expect("Failed to check instance status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    if let Ok(instances) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout_check) {
        assert!(instances.is_empty(), "Instance should be removed");
    }
}

#[test]
fn test_incus_container_remove_absent() {
    skip_without_incus!();

    let instance_name = "rash-test-container-absent";
    cleanup_instance(instance_name);

    let remove_script = format!(
        r#"
#!/usr/bin/env rash
- name: Remove non-existent instance
  incus:
    name: {}
    state: absent
"#,
        instance_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&remove_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for absent instance: {}",
        stdout
    );
}

#[test]
fn test_incus_container_already_stopped() {
    skip_without_incus!();

    let instance_name = "rash-test-container-already-stopped";
    cleanup_instance(instance_name);

    let _ = Command::new("incus")
        .args(["init", "images:alpine/3.19", instance_name])
        .output();

    let stop_script = format!(
        r#"
#!/usr/bin/env rash
- name: Stop already stopped instance
  incus:
    name: {}
    state: stopped
"#,
        instance_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&stop_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for already stopped instance: {}",
        stdout
    );

    cleanup_instance(instance_name);
}

#[test]
fn test_incus_container_with_config() {
    skip_without_incus!();

    let instance_name = "rash-test-container-config";
    cleanup_instance(instance_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create instance with config
  incus:
    name: {}
    image: images:alpine/3.19
    state: started
    config:
      limits.memory: 256MB
"#,
        instance_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("incus")
        .args(["config", "get", instance_name, "limits.memory"])
        .output()
        .expect("Failed to check instance config");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains("256MB"),
        "Config should be set: {}",
        stdout_check
    );

    cleanup_instance(instance_name);
}
