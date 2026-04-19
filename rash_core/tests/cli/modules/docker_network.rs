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

fn cleanup_network(name: &str) {
    let _ = Command::new("docker")
        .args(["network", "rm", "-f", name])
        .output();
}

#[test]
fn test_docker_network_create() {
    skip_without_docker!();

    let network_name = "rash-test-network-create";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create network
  docker_network:
    name: {}
"#,
        network_name
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
            "network",
            "ls",
            "--filter",
            &format!("name={}", network_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(stdout_check.contains(network_name), "Network should exist");

    cleanup_network(network_name);
}

#[test]
fn test_docker_network_create_with_subnet() {
    skip_without_docker!();

    let network_name = "rash-test-network-subnet";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create network with subnet
  docker_network:
    name: {}
    driver: bridge
    subnet: "172.28.0.0/16"
    gateway: "172.28.0.1"
"#,
        network_name
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
            "network",
            "inspect",
            "--format",
            "{{range .IPAM.Config}}{{.Subnet}}{{end}}",
            network_name,
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains("172.28.0.0/16"),
        "Network should have correct subnet: {}",
        stdout_check
    );

    cleanup_network(network_name);
}

#[test]
fn test_docker_network_create_internal() {
    skip_without_docker!();

    let network_name = "rash-test-network-internal";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create internal network
  docker_network:
    name: {}
    driver: bridge
    internal: true
"#,
        network_name
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
            "network",
            "inspect",
            "--format",
            "{{.Internal}}",
            network_name,
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains("true"),
        "Network should be internal: {}",
        stdout_check
    );

    cleanup_network(network_name);
}

#[test]
fn test_docker_network_remove() {
    skip_without_docker!();

    let network_name = "rash-test-network-remove";

    let _ = Command::new("docker")
        .args(["network", "create", network_name])
        .output();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove network
  docker_network:
    name: {}
    state: absent
"#,
        network_name
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
            "network",
            "ls",
            "--filter",
            &format!("name={}", network_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(network_name),
        "Network should be removed"
    );
}

#[test]
fn test_docker_network_remove_absent() {
    skip_without_docker!();

    let network_name = "rash-test-network-absent";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove non-existent network
  docker_network:
    name: {}
    state: absent
"#,
        network_name
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for absent network: {}",
        stdout
    );
}

#[test]
fn test_docker_network_check_mode() {
    skip_without_docker!();

    let network_name = "rash-test-network-check";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create network
  docker_network:
    name: {}
"#,
        network_name
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
            "network",
            "ls",
            "--filter",
            &format!("name={}", network_name),
            "--format",
            "{{.Name}}",
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout_check.contains(network_name),
        "Network should NOT be created in check mode"
    );
}

#[test]
fn test_docker_network_idempotent() {
    skip_without_docker!();

    let network_name = "rash-test-network-idempotent";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create network
  docker_network:
    name: {}
"#,
        network_name
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

    cleanup_network(network_name);
}

#[test]
fn test_docker_network_with_ip_range() {
    skip_without_docker!();

    let network_name = "rash-test-network-iprange";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create network with IP range
  docker_network:
    name: {}
    subnet: "172.29.0.0/16"
    ip_range: "172.29.0.0/24"
"#,
        network_name
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
            "network",
            "inspect",
            "--format",
            "{{range .IPAM.Config}}{{.IPRange}}{{end}}",
            network_name,
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains("172.29.0.0/24"),
        "Network should have correct IP range: {}",
        stdout_check
    );

    cleanup_network(network_name);
}

fn ipv6_available() -> bool {
    Command::new("docker")
        .args([
            "run",
            "--rm",
            "alpine:latest",
            "cat",
            "/proc/sys/net/ipv6/conf/all/disable_ipv6",
        ])
        .output()
        .map(|o| {
            if o.status.success() {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.trim() == "0"
            } else {
                false
            }
        })
        .unwrap_or(false)
}

macro_rules! skip_without_ipv6 {
    () => {
        if !ipv6_available() {
            eprintln!("Skipping test: IPv6 not available in Docker daemon");
            return;
        }
    };
}

#[test]
fn test_docker_network_ipv6() {
    skip_without_docker!();
    skip_without_ipv6!();

    let network_name = "rash-test-network-ipv6";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create IPv6 network
  docker_network:
    name: {}
    enable_ipv6: true
    subnet: "fd00:dead:beef::/48"
"#,
        network_name
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
            "network",
            "inspect",
            "--format",
            "{{.EnableIPv6}}",
            network_name,
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains("true"),
        "Network should have IPv6 enabled: {}",
        stdout_check
    );

    cleanup_network(network_name);
}

#[test]
fn test_docker_network_attachable() {
    skip_without_docker!();

    let network_name = "rash-test-network-attachable";
    cleanup_network(network_name);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create attachable network
  docker_network:
    name: {}
    driver: bridge
    attachable: true
"#,
        network_name
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
            "network",
            "inspect",
            "--format",
            "{{.Attachable}}",
            network_name,
        ])
        .output()
        .expect("Failed to check network");
    let stdout_check = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_check.contains("true"),
        "Network should be attachable: {}",
        stdout_check
    );

    cleanup_network(network_name);
}
