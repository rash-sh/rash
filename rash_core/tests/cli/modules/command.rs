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
