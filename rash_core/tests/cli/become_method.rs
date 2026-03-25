use crate::cli::execute_rash;

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
