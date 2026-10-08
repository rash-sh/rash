use crate::cli::modules::{run_test, run_tests};

use std::collections::HashMap;

#[test]
fn test_include_not_exists() {
    let script_text = r#"
#!/usr/bin/env rash
- name: File not exists
  include: file_not_exists.rh
        "#;

    let (stdout, stderr) = run_test(script_text, &[]);

    assert!(stdout.contains("- 1 to go - "));
    assert!(
        stderr.contains("[ERROR] Error reading file file_not_exists.rh: No such file or directory"),
        "{stderr}"
    );
}

#[test]
fn test_include() {
    let script_content = r#"#!/usr/bin/env rash
- assert:
    that:
      - rash.path == "{{ rash.dir }}/script.rh"

- name: Add lib
  include: "{{ rash.dir }}/lib.rh"

- assert:
    that:
      - rash.path == "{{ rash.dir }}/script.rh"
    "#;

    let lib_content = r#"
- assert:
    that:
      - rash.path == "{{ rash.dir }}/lib.rh"
    "#;

    let scripts = HashMap::from([("script.rh", script_content), ("lib.rh", lib_content)]);
    let (stdout, stderr) = run_tests(scripts, "script.rh", &[]);

    assert!(stdout.contains("script.rh:assert] - 3 to go - "));
    assert!(stdout.contains("lib.rh:assert] - 1 to go - "));
    assert!(stderr.is_empty());
}

#[test]
fn test_include_export_of_variable_never_set_fails() {
    let script_content = r#"
- include:
    file: "{{ rash.dir }}/lib.rh"
    export: [foo]
- debug:
    msg: after-include
    "#;
    let lib_content = r#"
- debug:
    msg: sets nothing
    "#;

    let scripts = HashMap::from([("script.rh", script_content), ("lib.rh", lib_content)]);
    let (stdout, stderr) = run_tests(scripts, "script.rh", &[]);

    assert!(
        stderr.contains("Included file did not define exported variable 'foo'"),
        "{stderr}"
    );
    assert!(!stdout.contains("after-include"), "{stdout}");
}

#[test]
fn test_include_reports_parse_error_of_mapping_form() {
    let script_content = r#"
- include: "{{ rash.dir }}/lib.rh"
    "#;
    let lib_content = r#"
tasks:
  - debug:
      msg: hi
    no_such_attribute: true
    "#;

    let scripts = HashMap::from([("script.rh", script_content), ("lib.rh", lib_content)]);
    let (_, stderr) = run_tests(scripts, "script.rh", &[]);

    assert!(stderr.contains("no_such_attribute"), "{stderr}");
    assert!(!stderr.contains("Expected a YAML sequence"), "{stderr}");
}
