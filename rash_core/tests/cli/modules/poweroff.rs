use crate::cli::modules::run_test;

#[test]
fn test_poweroff_check_mode_default() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Simulate poweroff in check mode
  poweroff:
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Would Poweroff") || stdout.contains("changed"));
}

#[test]
fn test_poweroff_check_mode_with_state() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Simulate shutdown in check mode
  poweroff:
    state: shutdown
    msg: Maintenance shutdown
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Would Shutdown") || stdout.contains("changed"));
}

#[test]
fn test_poweroff_check_mode_halt() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Simulate halt in check mode
  poweroff:
    state: halt
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("Would Halt") || stdout.contains("changed"));
}

#[test]
fn test_poweroff_check_mode_force() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Simulate forced poweroff in check mode
  poweroff:
    force: true
    msg: Forced shutdown
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("forced") || stdout.contains("changed"));
}

#[test]
fn test_poweroff_check_mode_with_delay() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Simulate delayed poweroff in check mode
  poweroff:
    delay: 300
    msg: Scheduled poweroff
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("300 seconds") || stdout.contains("changed"));
}

#[test]
fn test_poweroff_cancel_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Cancel scheduled poweroff in check mode
  poweroff:
    cancel: true
        "#
    .to_string();

    let args: &[&str] = &["--check"];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("changed") || stdout.contains("ok"));
}
