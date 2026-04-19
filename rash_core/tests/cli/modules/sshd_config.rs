use crate::cli::modules::run_test;

#[test]
fn test_sshd_config_add_option() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test sshd_config module add option
  sshd_config:
    option: PermitRootLogin
    value: "no"
    path: /tmp/test_sshd_config_add
"#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let config_file = std::path::Path::new("/tmp/test_sshd_config_add");
    assert!(config_file.exists(), "sshd_config file should exist");

    let content = std::fs::read_to_string(config_file).unwrap();
    assert!(
        content.contains("PermitRootLogin no"),
        "sshd_config should contain PermitRootLogin no"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_sshd_config_update_option() {
    let config_file = std::path::Path::new("/tmp/test_sshd_config_update");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(config_file, "Port 22\nPermitRootLogin yes\n").ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test sshd_config module update option
  sshd_config:
    option: PermitRootLogin
    value: "no"
    path: /tmp/test_sshd_config_update
"#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let content = std::fs::read_to_string(config_file).unwrap();
    assert!(
        content.contains("PermitRootLogin no"),
        "sshd_config should contain updated value"
    );
    assert!(
        content.contains("Port 22"),
        "sshd_config should preserve other options"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_sshd_config_no_change() {
    let config_file = std::path::Path::new("/tmp/test_sshd_config_no_change");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(config_file, "Port 22\nPermitRootLogin no\n").ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test sshd_config module no change
  sshd_config:
    option: PermitRootLogin
    value: "no"
    path: /tmp/test_sshd_config_no_change
"#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok' (no change), got: {}",
        stdout
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_sshd_config_remove_option() {
    let config_file = std::path::Path::new("/tmp/test_sshd_config_remove");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "Port 22\nPermitRootLogin no\nPasswordAuthentication no\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test sshd_config module remove option
  sshd_config:
    option: PermitRootLogin
    state: absent
    path: /tmp/test_sshd_config_remove
"#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let content = std::fs::read_to_string(config_file).unwrap();
    assert!(
        !content.contains("PermitRootLogin"),
        "sshd_config should not contain removed option"
    );
    assert!(
        content.contains("Port 22"),
        "sshd_config should preserve other options"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_sshd_config_match_block() {
    let config_file = std::path::Path::new("/tmp/test_sshd_config_match");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "Port 22\nPermitRootLogin no\n\nMatch User admin\n    PasswordAuthentication yes\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test sshd_config module match block
  sshd_config:
    option: PasswordAuthentication
    value: "no"
    match_criteria: User admin
    path: /tmp/test_sshd_config_match
"#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let content = std::fs::read_to_string(config_file).unwrap();
    assert!(
        content.contains("Match User admin"),
        "sshd_config should contain match block"
    );
    assert!(
        content.contains("PasswordAuthentication no"),
        "sshd_config should contain updated option in match block"
    );

    std::fs::remove_file(config_file).ok();
}
