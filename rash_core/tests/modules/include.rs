use crate::modules::{run_test, run_tests};

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
    assert!(stderr.contains("[ERROR] Error reading file: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }\n"));
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
