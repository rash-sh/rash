#[cfg(target_os = "linux")]
use crate::cli::modules::run_test;

#[cfg(target_os = "linux")]
#[test]
fn test_pids_pattern() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Find processes by pattern
  pids:
    pattern: ".*"
  register: result

- name: Verify pids is a list
  assert:
    that:
      - result.extra.pids is sequence
      - result.extra.pids | length > 0
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("ok"));
}

#[cfg(target_os = "linux")]
#[test]
fn test_pids_with_exclude() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Find all processes excluding rash
  pids:
    pattern: ".*"
    exclude:
      - rash
      - cargo
  register: result

- name: Verify processes were found
  assert:
    that:
      - result.extra.pids is sequence
        "#;

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(stderr.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn test_pids_user_filter() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Find processes by user
  pids:
    user: root
  register: result

- name: Verify result contains pids
  assert:
    that:
      - result.extra.pids is sequence
      - result.extra.processes is sequence
        "#;

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(stderr.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn test_pids_returns_process_info() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Find processes and check details
  pids:
    pattern: ".*"
  register: result

- name: Verify process details
  assert:
    that:
      - result.extra.processes[0].pid is number
      - result.extra.processes[0].name is string
      - result.extra.processes[0].user is string
      - result.extra.processes[0].command is string
        "#;

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(stderr.is_empty());
}
