use crate::cli::modules::run_test;

#[test]
fn test_fail_with_custom_message() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test fail module with custom message
  fail:
    msg: "Custom error message for testing"
        "#
    .to_string();

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Custom error message for testing"));
}

#[test]
fn test_fail_with_default_message() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test fail module with default message
  fail: {}
        "#
    .to_string();

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Failed as requested"));
}

#[test]
fn test_fail_conditional_with_when() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set a variable
  set_vars:
    should_fail: false

- name: This should not run
  fail:
    msg: "This should not appear"
  when: should_fail

- name: Set variable to true
  set_vars:
    should_fail: true

- name: This should run and fail
  fail:
    msg: "Conditional failure triggered"
  when: should_fail
        "#
    .to_string();

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(&script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Conditional failure triggered"));
    assert!(!stderr.contains("This should not appear"));
}
