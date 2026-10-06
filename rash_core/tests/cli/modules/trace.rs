use crate::cli::modules::run_test_with_env;

#[test]
fn test_trace_file_opens() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace file opens
  trace:
    probe: file_opens
    duration: "1s"
  register: files

- name: Verify events captured
  assert:
    that:
      - "{{ files.extra.events | length > 0 }}"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Traced for"), "stdout: {}", stdout);
}

#[test]
fn test_trace_process_exec() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace process execution
  trace:
    probe: process_exec
    duration: "1s"
  register: procs

- name: Verify events captured
  assert:
    that:
      - "{{ procs.extra.events | length > 0 }}"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Traced for"), "stdout: {}", stdout);
}

#[test]
fn test_trace_syscalls_with_filter() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace syscalls with filter
  trace:
    probe: syscalls
    filter: open,read
    duration: "1s"
  register: syscalls

- name: Verify events captured
  assert:
    that:
      - "{{ syscalls.extra.events | length > 0 }}"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Traced for"), "stdout: {}", stdout);
}

#[test]
fn test_trace_custom_expression() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace with custom expression
  trace:
    expr: 'tracepoint:syscalls:sys_enter_open { @[comm] = count(); }'
    duration: "1s"
  register: custom

- name: Verify events captured
  assert:
    that:
      - "{{ custom.extra.events | length > 0 }}"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Traced for"), "stdout: {}", stdout);
}

#[test]
fn test_trace_stats_included() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace file opens
  trace:
    probe: file_opens
    duration: "1s"
  register: files

- name: Verify stats
  debug:
    msg: "Total: {{ files.extra.stats.total }}, by comm: {{ files.extra.stats.by_comm }}"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Total:"), "stdout: {}", stdout);
}

#[test]
fn test_trace_network_connect() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace network connections
  trace:
    probe: network_connect
    duration: "1s"
  register: conns

- name: Verify events captured
  assert:
    that:
      - "{{ conns.extra.events | length > 0 }}"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Traced for"), "stdout: {}", stdout);
}

#[test]
fn test_trace_default_duration() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace with default duration
  trace:
    probe: file_opens
  register: files

- debug:
    msg: "Duration: {{ files.extra.duration_ms }}ms"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("Duration:"), "stdout: {}", stdout);
}

#[test]
fn test_trace_invalid_probe() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Trace with invalid probe
  trace:
    probe: invalid_probe
    duration: "1s"
    "#
    .to_string();

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test_with_env(&script_text, args, &[]);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("Invalid probe"));
}
