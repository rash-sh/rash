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

#[test]
fn test_docker_info_basic() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Get Docker info
  docker_info:
  register: docker
"#;

    let args = [];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("Docker information collected"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn test_docker_info_check_mode() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Get Docker info in check mode
  docker_info:
  register: docker
"#;

    let args = ["--check"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("Docker information collected"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn test_docker_info_no_version() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Get Docker info without version
  docker_info:
    get_version: false
  register: docker
"#;

    let args = [];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("Docker information collected"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn test_docker_info_no_info() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Get Docker info without system info
  docker_info:
    get_info: false
  register: docker
"#;

    let args = [];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("Docker information collected"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn test_docker_info_disk_usage() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Get Docker info with disk usage
  docker_info:
    get_disk_usage: true
  register: docker
"#;

    let args = [];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("Docker information collected"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn test_docker_info_only_disk_usage() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Get Docker disk usage only
  docker_info:
    get_version: false
    get_info: false
    get_disk_usage: true
  register: docker
"#;

    let args = [];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("Docker information collected"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn test_docker_info_not_available() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Get Docker info (may not be available)
  docker_info:
  register: docker
"#;

    let args = [];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    if docker_available() {
        assert!(
            stdout.contains("Docker information collected"),
            "stdout: {}",
            stdout
        );
    } else {
        assert!(
            stdout.contains("Docker is not available"),
            "stdout: {}",
            stdout
        );
    }
}
