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

macro_rules! skip_without_docker {
    () => {
        if !docker_available() {
            eprintln!("Skipping test: Docker not available");
            return;
        }
    };
}

fn cleanup_project(name: &str) {
    let _ = Command::new("docker")
        .args(["compose", "-p", name, "down", "--volumes", "--rmi", "all"])
        .output();
}

fn create_compose_file(path: &str) {
    fs::write(
        path,
        r#"
version: '3'
services:
  web:
    image: alpine:latest
    command: tail -f /dev/null
  db:
    image: alpine:latest
    command: tail -f /dev/null
"#,
    )
    .expect("Failed to create compose file");
}

#[test]
fn test_docker_compose_check_mode() {
    skip_without_docker!();

    let project_name = "rash-test-compose-check";
    cleanup_project(project_name);

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let compose_file = tmp_dir.path().join("docker-compose.yml");
    create_compose_file(compose_file.to_str().unwrap());

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not start project
  docker_compose:
    project_src: {}
    project_name: {}
    state: started
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
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
        .args(["compose", "-p", project_name, "ps", "-q"])
        .output()
        .expect("Failed to check project status");
    assert!(
        output.stdout.is_empty(),
        "Project should NOT be started in check mode"
    );

    cleanup_project(project_name);
}

#[test]
fn test_docker_compose_start() {
    skip_without_docker!();

    let project_name = "rash-test-compose-start";
    cleanup_project(project_name);

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let compose_file = tmp_dir.path().join("docker-compose.yml");
    create_compose_file(compose_file.to_str().unwrap());

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Start project
  docker_compose:
    project_src: {}
    project_name: {}
    state: started
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
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
        .args(["compose", "-p", project_name, "ps", "-q"])
        .output()
        .expect("Failed to check project status");
    assert!(!output.stdout.is_empty(), "Project should be started");

    cleanup_project(project_name);
}

#[test]
fn test_docker_compose_stop() {
    skip_without_docker!();

    let project_name = "rash-test-compose-stop";
    cleanup_project(project_name);

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let compose_file = tmp_dir.path().join("docker-compose.yml");
    create_compose_file(compose_file.to_str().unwrap());

    let start_script = format!(
        r#"
#!/usr/bin/env rash
- name: Start project first
  docker_compose:
    project_src: {}
    project_name: {}
    state: started
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&start_script, &args);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after start: {}",
        stderr
    );

    let stop_script = format!(
        r#"
#!/usr/bin/env rash
- name: Stop project
  docker_compose:
    project_src: {}
    project_name: {}
    state: stopped
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
    );

    let (stdout, stderr) = run_test(&stop_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args(["compose", "-p", project_name, "ps", "--format", "json"])
        .output()
        .expect("Failed to check project status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    let running = stdout_check.lines().any(|line| {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            json.get("State").and_then(|s| s.as_str()) == Some("running")
        } else {
            false
        }
    });
    assert!(!running, "Project should be stopped");

    cleanup_project(project_name);
}

#[test]
fn test_docker_compose_down() {
    skip_without_docker!();

    let project_name = "rash-test-compose-down";
    cleanup_project(project_name);

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let compose_file = tmp_dir.path().join("docker-compose.yml");
    create_compose_file(compose_file.to_str().unwrap());

    let start_script = format!(
        r#"
#!/usr/bin/env rash
- name: Start project first
  docker_compose:
    project_src: {}
    project_name: {}
    state: started
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&start_script, &args);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after start: {}",
        stderr
    );

    let down_script = format!(
        r#"
#!/usr/bin/env rash
- name: Remove project
  docker_compose:
    project_src: {}
    project_name: {}
    state: absent
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
    );

    let (stdout, stderr) = run_test(&down_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    let output = Command::new("docker")
        .args(["compose", "-p", project_name, "ps", "-q"])
        .output()
        .expect("Failed to check project status");
    assert!(output.stdout.is_empty(), "Project should be removed");

    cleanup_project(project_name);
}

#[test]
fn test_docker_compose_restart() {
    skip_without_docker!();

    let project_name = "rash-test-compose-restart";
    cleanup_project(project_name);

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let compose_file = tmp_dir.path().join("docker-compose.yml");
    create_compose_file(compose_file.to_str().unwrap());

    let start_script = format!(
        r#"
#!/usr/bin/env rash
- name: Start project first
  docker_compose:
    project_src: {}
    project_name: {}
    state: started
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&start_script, &args);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after start: {}",
        stderr
    );

    let restart_script = format!(
        r#"
#!/usr/bin/env rash
- name: Restart project
  docker_compose:
    project_src: {}
    project_name: {}
    state: restarted
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
    );

    let (stdout, stderr) = run_test(&restart_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    cleanup_project(project_name);
}

#[test]
fn test_docker_compose_specific_services() {
    skip_without_docker!();

    let project_name = "rash-test-compose-services";
    cleanup_project(project_name);

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let compose_file = tmp_dir.path().join("docker-compose.yml");
    create_compose_file(compose_file.to_str().unwrap());

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Start only web service
  docker_compose:
    project_src: {}
    project_name: {}
    state: started
    services:
      - web
"#,
        tmp_dir.path().to_str().unwrap(),
        project_name
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
        .args(["compose", "-p", project_name, "ps", "--format", "json"])
        .output()
        .expect("Failed to check project status");
    let stdout_check = String::from_utf8_lossy(&output.stdout);

    let services: Vec<String> = stdout_check
        .lines()
        .filter_map(|line| {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                json.get("Service")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();

    assert!(
        services.contains(&"web".to_string()),
        "web service should be started"
    );
    assert!(
        !services.contains(&"db".to_string()),
        "db service should not be started"
    );

    cleanup_project(project_name);
}
