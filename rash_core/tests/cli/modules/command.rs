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
