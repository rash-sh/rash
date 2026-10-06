use crate::cli::{execute_rash, execute_rash_with_env, running_as_root};

#[test]
fn test_become_method_sudo_command() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test command with sudo become
  command: echo "hello from sudo"
  become: true
  become_method: sudo
  become_user: root
  register: result
- debug:
    msg: "Command executed successfully"
"#
    .to_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, &script_text).unwrap();

    let args = ["--output", "raw", script_path.to_str().unwrap()];
    let (stdout, stderr) = execute_rash(&args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    // The internal task execution writes results to file, not stdout
    // We just verify no errors occurred
    assert!(
        stdout.contains("Command executed successfully") || stdout.is_empty(),
        "stdout should contain debug output or be empty: {}",
        stdout
    );
}

#[test]
fn test_become_method_sudo_file_module() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test file module with sudo become
  file:
    path: /tmp/rash_become_test.txt
    state: touch
  become: true
  become_method: sudo
  become_user: root
"#
    .to_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, &script_text).unwrap();

    let args = ["--output", "raw", script_path.to_str().unwrap()];
    let (stdout, stderr) = execute_rash(&args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    // File module with sudo become should succeed
    assert!(
        stdout.contains("/tmp/rash_become_test.txt") || stdout.is_empty(),
        "stdout should contain file path or be empty for changed: {}",
        stdout
    );

    // Cleanup
    let _ = std::fs::remove_file("/tmp/rash_become_test.txt");
}

#[test]
fn test_become_method_default_syscall() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test default become method
  debug:
    msg: "default method test"
  become: false
"#
    .to_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, &script_text).unwrap();

    let args = ["--output", "raw", script_path.to_str().unwrap()];
    let (stdout, stderr) = execute_rash(&args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("default method test"),
        "stdout should contain message: {}",
        stdout
    );
}

#[test]
fn test_become_exe_custom_path() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test custom become_exe
  command: echo "custom sudo"
  become: true
  become_method: sudo
  become_exe: sudo
  become_user: root
  register: result
- debug:
    msg: "Custom sudo test completed"
"#
    .to_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, &script_text).unwrap();

    let args = ["--output", "raw", script_path.to_str().unwrap()];
    let (stdout, stderr) = execute_rash(&args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    // Verify execution completed
    assert!(
        stdout.contains("Custom sudo test completed") || stdout.is_empty(),
        "stdout should contain debug output or be empty: {}",
        stdout
    );
}

#[test]
fn test_cli_become_method_flag() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test CLI become method
  command: echo "cli test"
  become: true
  register: result
- debug:
    msg: "CLI test completed"
"#
    .to_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, &script_text).unwrap();

    let args = [
        "--become",
        "--become-method",
        "sudo",
        "--output",
        "raw",
        script_path.to_str().unwrap(),
    ];
    let (stdout, stderr) = execute_rash(&args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    // Verify execution completed
    assert!(
        stdout.contains("CLI test completed") || stdout.is_empty(),
        "stdout should contain debug output or be empty: {}",
        stdout
    );
}

#[test]
fn test_become_password_task_parameter() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test become_password parameter
  command: echo "with password"
  become: true
  become_method: sudo
  become_user: root
  become_password: "test_password"
  register: result
- debug:
    msg: "Password test completed"
"#
    .to_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, &script_text).unwrap();

    let args = ["--output", "raw", script_path.to_str().unwrap()];
    let (stdout, stderr) = execute_rash(&args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    // Verify execution completed (mock doesn't actually check password)
    assert!(
        stdout.contains("Password test completed") || stdout.is_empty(),
        "stdout should contain debug output or be empty: {}",
        stdout
    );
}

#[test]
fn test_become_sudo_task_files_are_private_and_removed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_path = temp_dir.path().join("sudo.log");
    let fake_sudo = temp_dir.path().join("fake-sudo");
    std::fs::write(
        &fake_sudo,
        r#"#!/bin/sh
while [ "$1" != "--" ]; do shift; done
shift
{
  echo "$3"
  echo "$RASH_INTERNAL_RESULT_FILE"
  # Portable (GNU and BSD) permission strings, e.g. -rw-------
  ls -ln "$3" | cut -c1-10
  ls -ln "$RASH_INTERNAL_RESULT_FILE" | cut -c1-10
} > "$RASH_TEST_SUDO_LOG"
exec "$@"
"#,
    )
    .unwrap();
    std::fs::set_permissions(
        &fake_sudo,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();

    let script_text = r#"
#!/usr/bin/env rash
- command: echo "{{ secret }}"
  vars:
    secret: top-secret-value
  become: true
  become_method: sudo
  become_user: root
  register: result
- assert:
    that:
      - result.stdout == "top-secret-value\n"
- debug:
    msg: sudo-files-ok
"#;
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, script_text).unwrap();

    let args = [
        "--output",
        "raw",
        "--become-exe",
        fake_sudo.to_str().unwrap(),
        script_path.to_str().unwrap(),
    ];
    let log = log_path.to_str().unwrap();
    let (stdout, stderr) = execute_rash_with_env(&args, &[("RASH_TEST_SUDO_LOG", log)]);

    assert!(stdout.contains("sudo-files-ok"), "stderr: {stderr}");
    let log_content = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = log_content.lines().collect();
    assert_eq!(&lines[2..], ["-rw-------", "-rw-------"], "{log_content}");
    for file in &lines[..2] {
        assert!(
            !std::path::Path::new(file).exists(),
            "{file} was not removed"
        );
    }
}

/// Run a script with the mocks (e.g. `sudo`) first in PATH, returning exit code and output.
fn run_script_status(script_text: &str, args: &[&str]) -> (Option<i32>, String) {
    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("test.rh");
    std::fs::write(&script_path, script_text).unwrap();
    let mocks = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    let path = std::env::join_paths(
        std::iter::once(mocks).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rash"))
        .args(args)
        .arg(&script_path)
        .env("PATH", path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    (output.status.code(), text)
}

const TEMPLATED_EXIT: &str = r#"
- set_vars:
    wanted: "{{ 3 + 4 }}"
- block:
    - meta:
        action: exit
        code: "{{ wanted }}"
  become: true
- debug:
    msg: unreachable-after-exit
"#;

#[test]
fn test_meta_exit_code_propagates_with_become() {
    for args in [
        &[][..],
        &["--become"][..],
        &["--become", "--become-method", "sudo"][..],
    ] {
        let (code, output) = run_script_status(TEMPLATED_EXIT, args);
        assert_eq!(code, Some(7), "{args:?}: {output}");
        assert!(!output.contains("unreachable-after-exit"), "{output}");
    }
}

#[test]
fn test_block_children_inherit_become_and_escalate_individually() {
    let script_text = r#"
- block:
    - command: echo escalated
      register: inner
  become: true
  become_method: sudo
- assert:
    that:
      - inner.stdout == "escalated\n"
- debug:
    msg: block-become-ok
"#;
    let (code, output) = run_script_status(script_text, &[]);
    assert_eq!(code, Some(0), "{output}");
    assert!(output.contains("block-become-ok"), "{output}");
}

#[test]
fn test_failed_become_child_never_continues_script() {
    let script_text = r#"
- command: echo hi
  become: true
  become_user: nobody
  ignore_errors: true
- debug:
    msg: after-become-task
"#;
    let (code, output) = run_script_status(script_text, &[]);
    assert_eq!(code, Some(0), "{output}");
    assert_eq!(output.matches("after-become-task").count(), 1, "{output}");
}

/// Switching to another user needs root: skipped otherwise.
#[test]
fn test_as_root_syscall_become_runs_modules_as_user_with_its_own_groups() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !running_as_root("test_as_root_syscall_become_runs_modules_as_user_with_its_own_groups") {
        return;
    }
    let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    let dest = dir.path().join("owned-by-nobody");
    let script_text = format!(
        r#"
- command: id -u
  become: true
  become_user: nobody
  register: uid
- command: id -G
  become: true
  become_user: nobody
  register: groups
- copy:
    content: written as nobody
    dest: {}
  become: true
  become_user: nobody
- debug:
    msg: "uid=[{{{{ uid.stdout | trim }}}}] groups=[{{{{ groups.stdout | trim }}}}]"
"#,
        dest.display()
    );

    let (code, output) = run_script_status(&script_text, &[]);

    assert_eq!(code, Some(0), "{output}");
    assert!(
        output.contains(&format!("uid=[{}]", nobody.uid)),
        "{output}"
    );
    let groups = output
        .split("groups=[")
        .nth(1)
        .unwrap()
        .split(']')
        .next()
        .unwrap();
    // initgroups: root's supplementary groups are not kept.
    assert!(!groups.split_whitespace().any(|gid| gid == "0"), "{output}");
    assert_eq!(std::fs::metadata(&dest).unwrap().uid(), nobody.uid.as_raw());
}

#[test]
fn test_ignored_become_failure_is_reported_once() {
    let script_text = r#"
- command: sh -c 'echo become-child-failed >&2; exit 3'
  become: true
  become_method: sudo
  ignore_errors: true
  register: failed_in_child
- assert:
    that:
      - failed_in_child is failed
      - failed_in_child.rc == 3
"#;
    let (code, output) = run_script_status(script_text, &["--output", "raw"]);
    assert_eq!(code, Some(0), "{output}");
    assert_eq!(output.matches("become-child-failed").count(), 1, "{output}");
}
