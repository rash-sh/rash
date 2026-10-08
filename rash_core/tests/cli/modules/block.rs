use crate::cli::modules::run_test;

#[test]
fn test_block_mapping_form_renders_children_lazily() {
    let script_text = r#"
#!/usr/bin/env rash
- block:
    tasks:
      - command: echo registered-value
        register: first
      - debug:
          msg: "child sees {{ first.stdout | trim }} with {{ prefix }}"
      - set_vars:
          from_loop: "{{ item }}"
        loop: [loop-a, loop-b]
    defaults:
      vars:
        prefix: "{{ 'default-' ~ 'prefix' }}"
- assert:
    that:
      - from_loop == "loop-b"
- debug:
    msg: block-mapping-ok
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(
        stdout.contains("child sees registered-value with default-prefix"),
        "stderr: {stderr}"
    );
    assert!(stdout.contains("block-mapping-ok"), "stderr: {stderr}");
}

#[test]
fn test_block_mapping_form_inside_loop_uses_item() {
    let script_text = r#"
#!/usr/bin/env rash
- block:
    tasks:
      - debug:
          msg: "outer item {{ item }}"
  loop: [first-item, second-item]
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("outer item first-item"), "stderr: {stderr}");
    assert!(
        stdout.contains("outer item second-item"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_rescue_and_always_inherit_check_mode_of_their_task() {
    let dir = tempfile::tempdir().unwrap();
    let script_text = format!(
        r#"
- block:
    - fail:
        msg: main failed
  check_mode: true
  rescue:
    - command: touch {dir}/rescue-ran
  always:
    - command: touch {dir}/always-ran
- debug:
    msg: check-mode-sections-ok
"#,
        dir = dir.path().display()
    );

    let (stdout, stderr) = run_test(&script_text, &[]);

    assert!(
        stdout.contains("check-mode-sections-ok"),
        "stderr: {stderr}"
    );
    assert!(stdout.contains("Would run: touch"), "{stdout}");
    assert!(!dir.path().join("rescue-ran").exists());
    assert!(!dir.path().join("always-ran").exists());
}

#[test]
fn test_rescue_and_always_see_task_vars() {
    let script_text = r#"
- block:
    - fail:
        msg: "failed with {{ scoped }}"
  vars:
    scoped: block-var
  rescue:
    - debug:
        msg: "rescue sees {{ scoped }}"
  always:
    - debug:
        msg: "always sees {{ scoped }}"
"#;

    let (stdout, stderr) = run_test(script_text, &[]);

    assert!(stdout.contains("rescue sees block-var"), "stderr: {stderr}");
    assert!(stdout.contains("always sees block-var"), "stderr: {stderr}");
}

#[test]
fn test_ignored_failure_with_always_is_reported_as_ignored() {
    let script_text = r#"
tasks:
  - block:
      - command: "true"
      - fail:
          msg: main failed
    ignore_errors: true
    register: outcome
    notify: report
    rescue:
      - debug:
          msg: rescue-ran
    always:
      - debug:
          msg: always-ran
  - assert:
      that:
        - outcome is failed
  - debug:
      msg: after-ignored
handlers:
  - name: report
    debug:
      msg: handler-ran
"#;

    let (stdout, stderr) = run_test(script_text, &[]);

    assert!(stdout.contains("always-ran"), "stderr: {stderr}");
    assert!(stdout.contains("[ignoring error] main failed"), "{stdout}");
    assert!(!stdout.contains("rescue-ran"), "{stdout}");
    assert!(!stdout.contains("handler-ran"), "{stdout}");
    assert!(stdout.contains("after-ignored"), "stderr: {stderr}");
}
