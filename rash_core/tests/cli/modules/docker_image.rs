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

fn cleanup_image(name: &str) {
    let _ = Command::new("docker")
        .args(["image", "rm", "-f", name])
        .output();
}

fn create_test_dockerfile(dir: &std::path::Path) {
    let dockerfile = r#"FROM alpine:latest
LABEL test="rash-integration"
RUN echo "test" > /test.txt
"#;
    fs::write(dir.join("Dockerfile"), dockerfile).expect("Failed to write Dockerfile");
}

#[test]
fn test_docker_image_pull() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "alpine:3.19";
    cleanup_image(image_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Pull image
  docker_image:
    name: {}
    source: pull
"#,
        image_name
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
        .args(["image", "inspect", "--format", "{{.Id}}", image_name])
        .output()
        .expect("Failed to check image");
    assert!(output.status.success(), "Image should exist");

    cleanup_image(image_name);
}

#[test]
fn test_docker_image_pull_idempotent() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "alpine:3.18";
    cleanup_image(image_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Pull image
  docker_image:
    name: {}
    source: pull
"#,
        image_name
    );

    let args = ["--diff"];
    let (stdout1, stderr1) = run_test(&script_text, &args);
    assert!(stderr1.is_empty(), "stderr should be empty: {}", stderr1);
    assert!(
        stdout1.contains("changed"),
        "First run should show changed: {}",
        stdout1
    );

    let (_stdout2, stderr2) = run_test(&script_text, &args);
    assert!(stderr2.is_empty(), "stderr should be empty: {}", stderr2);

    cleanup_image(image_name);
}

#[test]
fn test_docker_image_remove() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "alpine:3.17";

    let _ = Command::new("docker").args(["pull", image_name]).output();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove image
  docker_image:
    name: {}
    state: absent
"#,
        image_name
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
        .args(["image", "inspect", "--format", "{{.Id}}", image_name])
        .output()
        .expect("Failed to check image");
    assert!(!output.status.success(), "Image should not exist");
}

#[test]
fn test_docker_image_build() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "rash-test-image:build";
    cleanup_image(image_name);

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    create_test_dockerfile(temp_dir.path());

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Build image
  docker_image:
    name: rash-test-image
    tag: build
    source: build
    build:
      path: {}
"#,
        temp_dir.path().to_str().unwrap()
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
        .args(["image", "inspect", "--format", "{{.Id}}", image_name])
        .output()
        .expect("Failed to check image");
    assert!(output.status.success(), "Image should exist");

    cleanup_image(image_name);
}

#[test]
fn test_docker_image_build_with_args() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "rash-test-image:build-args";
    cleanup_image(image_name);

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let dockerfile = r#"ARG VERSION=latest
FROM alpine:${VERSION}
LABEL test="rash-integration"
"#;
    fs::write(temp_dir.path().join("Dockerfile"), dockerfile).expect("Failed to write Dockerfile");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Build image with args
  docker_image:
    name: rash-test-image
    tag: build-args
    source: build
    build:
      path: {}
      args:
        VERSION: "3.19"
"#,
        temp_dir.path().to_str().unwrap()
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
        .args(["image", "inspect", "--format", "{{.Id}}", image_name])
        .output()
        .expect("Failed to check image");
    assert!(output.status.success(), "Image should exist");

    cleanup_image(image_name);
}

#[test]
fn test_docker_image_force_pull() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "alpine:3.19";

    let _ = Command::new("docker").args(["pull", image_name]).output();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Force pull image
  docker_image:
    name: {}
    source: pull
    force_source: true
"#,
        image_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' with force_source: {}",
        stdout
    );

    cleanup_image(image_name);
}

#[test]
fn test_docker_image_check_mode() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "alpine:3.16";
    cleanup_image(image_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not pull image
  docker_image:
    name: {}
    source: pull
"#,
        image_name
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
        .args(["image", "inspect", "--format", "{{.Id}}", image_name])
        .output()
        .expect("Failed to check image");
    assert!(
        !output.status.success(),
        "Image should NOT be pulled in check mode"
    );
}

#[test]
fn test_docker_image_local_source() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "alpine:3.20";

    let _ = Command::new("docker").args(["pull", image_name]).output();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check local image exists
  docker_image:
    name: {}
    source: local
"#,
        image_name
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);

    cleanup_image(image_name);
}

#[test]
fn test_docker_image_local_source_not_found() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "nonexistent-image:nonexistent-tag";
    cleanup_image(image_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check local image exists (should fail)
  docker_image:
    name: {}
    source: local
"#,
        image_name
    );

    let args = ["--output", "raw"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.contains("not found locally") || stderr.contains("Error"),
        "stderr should contain error: {}",
        stderr
    );
}

#[test]
fn test_docker_image_with_tag() {
    if !docker_available() {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    let image_name = "alpine";
    let tag = "latest";
    let full_name = format!("{}:{}", image_name, tag);
    cleanup_image(&full_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Pull image with tag parameter
  docker_image:
    name: {}
    tag: {}
    source: pull
"#,
        image_name, tag
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
        .args(["image", "inspect", "--format", "{{.Id}}", &full_name])
        .output()
        .expect("Failed to check image");
    assert!(
        output.status.success(),
        "Image should exist with correct tag"
    );

    cleanup_image(&full_name);
}
