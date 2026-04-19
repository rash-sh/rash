use crate::cli::modules::run_test;

fn tailscale_available() -> bool {
    std::process::Command::new("tailscale")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_tailscale_invalid_no_auth_key() {
    if !tailscale_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Connect without auth key
  tailscale:
    state: up
        "#
    .to_string();

    let args = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("auth_key is required"));
}

#[test]
fn test_tailscale_check_mode_down() {
    if !tailscale_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Disconnect in check mode
  tailscale:
    state: down
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.contains("ok:"));
}

#[test]
fn test_tailscale_check_mode_logout() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Logout in check mode
  tailscale:
    state: logout
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_tailscale_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid field
  tailscale:
    state: up
    nonexistent: value
        "#
    .to_string();

    let args = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_tailscale_invalid_state() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid state
  tailscale:
    state: reconnect
        "#
    .to_string();

    let args = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(!stderr.is_empty());
}
