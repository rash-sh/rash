use std::process::Command;

use crate::cli::modules::run_test;

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cleanup_container(name: &str) {
    let _ = Command::new("docker").args(["rm", "-f", name]).output();
}

fn create_running_container(name: &str) {
    let _ = Command::new("docker")
        .args(["pull", "alpine:latest"])
        .output();
    let _ = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            name,
            "alpine:latest",
            "tail",
            "-f",
            "/dev/null",
        ])
        .output();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let output = Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("name=^{}$", name),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("Failed to check container status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(name) {
        eprintln!("Warning: Container {} is not running after creation", name);
    }
}

#[test]
fn test_docker_container_check_mode() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let container_name = "rash-test-container-check";
    cleanup_container(container_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create container
  docker_container:
    name: {}
    image: alpine:latest
    state: started
"#,
        container_name
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
            "ps",
            "-a",
            "--filter",
            &format!("name=^{}$", container_name),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("Failed to check container status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(container_name),
        "Container should NOT be created in check mode"
    );
}

#[test]
fn test_docker_container_stop() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let container_name = "rash-test-container-stop";
    cleanup_container(container_name);
    create_running_container(container_name);

    let stop_script = format!(
        r#"
#!/usr/bin/env rash
- name: Stop container
  docker_container:
    name: {}
    state: stopped
"#,
        container_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&stop_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name=^{}$", container_name),
            "--format",
            "{{.Status}}",
        ])
        .output()
        .expect("Failed to check container status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains("Up"),
        "Container should not be running"
    );

    cleanup_container(container_name);
}

#[test]
fn test_docker_container_remove() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let container_name = "rash-test-container-remove";
    cleanup_container(container_name);
    create_running_container(container_name);

    let stop_script = format!(
        r#"
#!/usr/bin/env rash
- name: Stop container first
  docker_container:
    name: {}
    state: stopped
"#,
        container_name
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
- name: Remove container
  docker_container:
    name: {}
    state: absent
"#,
        container_name
    );

    let (stdout, stderr) = run_test(&remove_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name=^{}$", container_name),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("Failed to check container status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(container_name),
        "Container should be removed"
    );
}

#[test]
fn test_docker_container_restart() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let container_name = "rash-test-container-restart";
    cleanup_container(container_name);
    create_running_container(container_name);

    let restart_script = format!(
        r#"
#!/usr/bin/env rash
- name: Restart container
  docker_container:
    name: {}
    state: restarted
"#,
        container_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&restart_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    cleanup_container(container_name);
}

#[test]
fn test_docker_container_force_remove() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let container_name = "rash-test-container-force";
    cleanup_container(container_name);
    create_running_container(container_name);

    let remove_script = format!(
        r#"
#!/usr/bin/env rash
- name: Force remove running container
  docker_container:
    name: {}
    state: absent
    force: true
"#,
        container_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&remove_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name=^{}$", container_name),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("Failed to check container status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(container_name),
        "Container should be removed"
    );
}

#[test]
fn test_docker_container_remove_absent() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let container_name = "rash-test-container-absent";
    cleanup_container(container_name);

    let remove_script = format!(
        r#"
#!/usr/bin/env rash
- name: Remove non-existent container
  docker_container:
    name: {}
    state: absent
"#,
        container_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&remove_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for absent container: {}",
        stdout
    );
}

#[test]
fn test_docker_container_already_stopped() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let container_name = "rash-test-container-already-stopped";
    cleanup_container(container_name);

    let _ = Command::new("docker")
        .args([
            "run",
            "--name",
            container_name,
            "alpine:latest",
            "echo",
            "test",
        ])
        .output();

    let stop_script = format!(
        r#"
#!/usr/bin/env rash
- name: Stop already stopped container
  docker_container:
    name: {}
    state: stopped
"#,
        container_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&stop_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for already stopped container: {}",
        stdout
    );

    cleanup_container(container_name);
}
