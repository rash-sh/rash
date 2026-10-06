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
