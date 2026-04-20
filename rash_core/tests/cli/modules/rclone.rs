use crate::cli::modules::run_test;

fn rclone_available() -> bool {
    std::process::Command::new("rclone")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_rclone_parse_params() {
    if !rclone_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Test rclone params parsing
  rclone:
    command: sync
    source: /data/backup
    dest: s3:my-bucket/backup
  check_mode: true
"#;

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
}

#[test]
fn test_rclone_parse_params_with_options() {
    if !rclone_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: Test rclone with options
  rclone:
    command: copy
    source: local:files
    dest: s3:bucket/files
    dry_run: true
  check_mode: true
"#;

    let args = ["--check", "--diff"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
}

#[test]
fn test_rclone_ls_command() {
    if !rclone_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: List remote contents
  rclone:
    command: ls
    source: /tmp
"#;

    let args = ["--diff"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
}

#[test]
fn test_rclone_invalid_command() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid command
  rclone:
    command: invalid
    source: /data
"#;

    let args = ["--diff"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(stderr.contains("Invalid command"), "stderr: {}", stderr);
}

#[test]
fn test_rclone_missing_dest_for_sync() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Missing dest
  rclone:
    command: sync
    source: /data
"#;

    let args = ["--diff"];
    let (_, stderr) = run_test(script_text, &args);

    assert!(stderr.contains("requires 'dest'"), "stderr: {}", stderr);
}

#[test]
fn test_rclone_register_result() {
    if !rclone_available() {
        return;
    }

    let script_text = r#"
#!/usr/bin/env rash
- name: List and register
  rclone:
    command: ls
    source: /tmp
  register: result
- debug:
    msg: "{{ result.extra }}"
"#;

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("cmd"),
        "stdout should contain 'cmd': {}",
        stdout
    );
}
