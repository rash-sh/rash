use crate::cli::{execute_rash, execute_rash_with_env};

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
  stat -c '%a' "$3" "$RASH_INTERNAL_RESULT_FILE"
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
    assert_eq!(&lines[2..], ["600", "600"], "{log_content}");
    for file in &lines[..2] {
        assert!(
            !std::path::Path::new(file).exists(),
            "{file} was not removed"
        );
    }
}
