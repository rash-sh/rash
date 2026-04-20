use std::process::Command;

use crate::cli::modules::{docker_test_lock, run_test};

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
        let _lock = docker_test_lock();
    };
}

#[test]
fn test_docker_prune_containers() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Prune stopped containers
  docker_prune:
    containers: true
"#;

    let args = ["--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed") || stdout.contains("ok"),
        "stdout should contain 'changed' or 'ok': {}",
        stdout
    );
}

#[test]
fn test_docker_prune_all() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Prune all Docker resources
  docker_prune:
    all: true
"#;

    let args = ["--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );
}

#[test]
fn test_docker_prune_multiple_types() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Prune multiple resource types
  docker_prune:
    containers: true
    images: true
    volumes: true
"#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
}

#[test]
fn test_docker_prune_check_mode() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Check mode - should not prune
  docker_prune:
    containers: true
    images: true
"#;

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should show changed in check mode: {}",
        stdout
    );
}

#[test]
fn test_docker_prune_no_options() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Prune without options
  docker_prune: {}
"#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
}
