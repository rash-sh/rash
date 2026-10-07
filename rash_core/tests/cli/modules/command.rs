use crate::cli::modules::run_test;

#[test]
fn test_command_transfer_pid_inherits_stdout() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Replace rash with the command
  command:
    cmd: echo transferred-output
    transfer_pid: true

- name: Never executed
  debug:
    msg: unreachable-task
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("transferred-output"), "stderr: {stderr}");
    assert!(!stdout.contains("unreachable-task"));
}

#[test]
fn test_command_transfer_pid_rejects_stdin() {
    let script_text = r#"
#!/usr/bin/env rash
- command:
    cmd: cat
    stdin: data
    transfer_pid: true
        "#;

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(stderr.contains("stdin cannot be combined with transfer_pid"));
}

#[test]
fn test_command_transfer_pid_cmd_execs_program_directly() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("script.rh");
    std::fs::write(
        &script,
        r#"
- command:
    cmd: sh -c 'echo "pid=$$"' 'literal $$ arg'
    transfer_pid: true
"#,
    )
    .unwrap();
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_rash"))
        .arg(&script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let rash_pid = child.id();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The program itself replaced rash: no `/bin/sh -c` wrapper kept the PID.
    assert!(
        stdout.contains(&format!("pid={rash_pid}")),
        "stdout: {stdout}, stderr: {stderr}"
    );
    assert!(output.status.success(), "stderr: {stderr}");
}

#[test]
fn test_command_transfer_pid_cmd_is_not_shell_expanded() {
    let script_text = r#"
#!/usr/bin/env rash
- command:
    cmd: printf '%s|' "$HOME" 'two words'
    transfer_pid: true
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("$HOME|two words|"), "stderr: {stderr}");
}

/// A failed `exec` leaves Rash with the program's chdir, stdio and signal setup applied:
/// it must stop with status 1, like `sh`, instead of running later tasks.
#[test]
fn test_command_transfer_pid_failed_exec_exits_rash() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("script.rh");
    std::fs::write(
        &script,
        r#"
- command:
    cmd: /nonexistent/rash-test-program
    transfer_pid: true
  ignore_errors: true
  rescue:
    - debug:
        msg: unreachable-rescue
  always:
    - debug:
        msg: unreachable-always
- debug:
    msg: unreachable-after-failed-exec
"#,
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rash"))
        .arg(&script)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(!stdout.contains("unreachable"), "{stdout}");
    assert!(!stderr.contains("unreachable"), "{stderr}");
    assert!(
        stderr.contains("/nonexistent/rash-test-program"),
        "{stderr}"
    );
    assert!(stderr.contains("transfer_pid failed"), "{stderr}");
}

/// `-vv` traces each task as parsed: never the params of a `no_log` one.
#[test]
fn test_command_no_log_hides_params_from_trace_logs() {
    let script_text = r#"
#!/usr/bin/env rash
- name: secret command
  command:
    argv: [echo, literal-hunter2]
  no_log: true
  register: secret
# Checked without quoting the secret: the params of this task are traced too.
- assert:
    that:
      - secret.rc == 0
      - secret.stdout | trim | length == 15
- debug:
    msg: no-log-trace-ok
        "#;

    let args: &[&str] = &["-vv"];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("no-log-trace-ok"), "stderr: {stderr}");
    assert!(!stdout.contains("hunter2"), "stdout: {stdout}");
    assert!(!stderr.contains("hunter2"), "stderr: {stderr}");
}

#[test]
fn test_command_no_log_failure_hides_stderr() {
    let script_text = r#"
#!/usr/bin/env rash
- name: secret failure
  command:
    argv: [sh, -c, "echo leaked-secret >&2; exit 3"]
  no_log: true
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(!stdout.contains("leaked-secret"), "stdout: {stdout}");
    assert!(!stderr.contains("leaked-secret"), "stderr: {stderr}");
    assert!(stderr.contains("'no_log: true' was specified"), "{stderr}");
}

#[test]
fn test_command_no_log_registered_result_keeps_data() {
    let script_text = r#"
#!/usr/bin/env rash
- command:
    argv: [sh, -c, "echo kept-data >&2; exit 3"]
  no_log: true
  ignore_errors: true
  register: secret
- assert:
    that:
      - secret.failed
      - "'kept-data' in secret.stderr"
- debug:
    msg: no-log-register-ok
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("no-log-register-ok"), "stderr: {stderr}");
    assert!(!stdout.contains("kept-data") && !stderr.contains("kept-data"));
}

#[test]
fn test_command_loop_registers_results_and_failure_of_any_item() {
    let script_text = r#"
#!/usr/bin/env rash
- command:
    argv: [sh, -c, "echo {{ item }}; test {{ item }} != b"]
  loop: [a, b, c]
  register: r
  ignore_errors: true
- assert:
    that:
      - r is failed
      - r.failed
      - r.error == "command exited with code 1"
      - r.stdout == "c\n"
      - r.item == "c"
      - r.results | length == 3
      - r.results[0] is succeeded
      - r.results[1] is failed
      - r.results[1].item == "b"
      - r.results[1].rc == 1
      - r.results[2].stdout == "c\n"
- debug:
    msg: loop-results-ok
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("loop-results-ok"), "stderr: {stderr}");
}

#[test]
fn test_command_until_sees_task_vars_and_skips_with_when() {
    let script_text = r#"
#!/usr/bin/env rash
- command: echo hi
  vars: {want: 0}
  register: r
  until: r.rc == want
  retries: 0
- command: echo never
  when: false
  register: skipped
  until: skipped.rc == 0
  retries: 1
- assert:
    that:
      - r.rc == 0
      - skipped is not defined
- debug:
    msg: until-context-ok
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("until-context-ok"), "stderr: {stderr}");
}

#[test]
fn test_command_templated_retries_is_a_parse_error() {
    let script_text = r#"
#!/usr/bin/env rash
- set_vars: {n: 0}
- command: echo hi
  until: false
  retries: "{{ n }}"
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(!stdout.contains("hi"), "stdout: {stdout}");
    assert!(
        stderr.contains(r#"retries must be an integer, templates are not supported: "{{ n }}""#),
        "stderr: {stderr}"
    );
}
