use crate::cli::modules::run_test;

#[test]
fn test_reboot_check_required() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Check if reboot is required
  reboot:
    check_required: true
  register: reboot_status

- name: Display reboot status
  debug:
    msg: "Reboot required: {{ reboot_status.reboot_required }}"
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Reboot required:") || stdout.contains("ok"));
}

#[test]
fn test_reboot_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Simulate reboot in check mode
  reboot:
    msg: Test reboot message
    delay: 10
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Would reboot") || stdout.contains("changed"));
}

#[test]
fn test_reboot_with_method() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Simulate reboot with specific method in check mode
  reboot:
    method: shutdown
    msg: Maintenance reboot
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Would reboot") || stdout.contains("changed"));
}

#[test]
fn test_reboot_cancel_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Cancel scheduled reboot in check mode
  reboot:
    cancel: true
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("changed") || stdout.contains("ok"));
}
