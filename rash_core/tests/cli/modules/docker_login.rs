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

fn logout_from_registry(registry: Option<&str>) {
    if let Some(reg) = registry {
        let _ = Command::new("docker").args(["logout", reg]).output();
    } else {
        let _ = Command::new("docker").args(["logout"]).output();
    }
}

#[test]
fn test_docker_login_missing_credentials() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Login without username
  docker_login:
    password: secret
"#;

    let args = ["--output", "raw"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(
        stderr.contains("username is required") || stderr.contains("Error"),
        "stderr should contain error about username: {}",
        stderr
    );
}

#[test]
fn test_docker_login_missing_password() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Login without password
  docker_login:
    username: myuser
"#;

    let args = ["--output", "raw"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(
        stderr.contains("password is required") || stderr.contains("Error"),
        "stderr should contain error about password: {}",
        stderr
    );
}

#[test]
fn test_docker_login_logout() {
    skip_without_docker!();

    logout_from_registry(None);

    let script_text = r#"
#!/usr/bin/env rash
- name: Logout from Docker Hub
  docker_login:
    state: absent
"#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
}

#[test]
fn test_docker_login_logout_private_registry() {
    skip_without_docker!();

    logout_from_registry(Some("registry.example.com"));

    let script_text = r#"
#!/usr/bin/env rash
- name: Logout from private registry
  docker_login:
    registry: registry.example.com
    state: absent
"#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
}

#[test]
fn test_docker_login_check_mode() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Check mode login
  docker_login:
    username: testuser
    password: testpassword
    registry: registry.example.com
"#;

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );
}

#[test]
fn test_docker_login_check_mode_logout() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Check mode logout
  docker_login:
    registry: registry.example.com
    state: absent
"#;

    let args = ["--check", "--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
}

#[test]
fn test_docker_login_with_email() {
    skip_without_docker!();

    logout_from_registry(Some("registry.example.com"));

    let script_text = r#"
#!/usr/bin/env rash
- name: Login with email (check mode)
  docker_login:
    registry: registry.example.com
    username: testuser
    password: testpassword
    email: test@example.com
"#;

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );
}

#[test]
fn test_docker_login_reauthorize() {
    skip_without_docker!();

    let script_text = r#"
#!/usr/bin/env rash
- name: Login with reauthorize (check mode)
  docker_login:
    registry: registry.example.com
    username: testuser
    password: testpassword
    reauthorize: true
"#;

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );
}
