use crate::cli::modules::run_test;

#[test]
fn test_ssh_config_add() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test ssh_config module add entry
  ssh_config:
    host: github.com
    options:
      hostname: github.com
      user: git
      identityfile: ~/.ssh/github_key
    ssh_config_file: /tmp/test_ssh_config_add
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

    let config_file = std::path::Path::new("/tmp/test_ssh_config_add");
    assert!(config_file.exists(), "ssh_config file should exist");

    let content = std::fs::read_to_string(config_file).unwrap();
    assert!(
        content.contains("Host github.com"),
        "ssh_config should contain Host github.com"
    );
    assert!(
        content.contains("hostname github.com"),
        "ssh_config should contain hostname option"
    );
    assert!(
        content.contains("user git"),
        "ssh_config should contain user option"
    );
    assert!(
        content.contains("identityfile"),
        "ssh_config should contain identityfile option"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_ssh_config_add_existing_no_change() {
    let config_file = std::path::Path::new("/tmp/test_ssh_config_no_change");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "Host github.com\n    hostname github.com\n    user git\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test ssh_config module add existing entry
  ssh_config:
    host: github.com
    options:
      hostname: github.com
      user: git
    ssh_config_file: /tmp/test_ssh_config_no_change
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
fn test_ssh_config_update_option() {
    let config_file = std::path::Path::new("/tmp/test_ssh_config_update");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "Host github.com\n    hostname github.com\n    user olduser\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test ssh_config module update option
  ssh_config:
    host: github.com
    options:
      hostname: github.com
      user: git
      port: 22
    ssh_config_file: /tmp/test_ssh_config_update
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
        content.contains("user git"),
        "ssh_config should contain updated user"
    );
    assert!(
        content.contains("port 22"),
        "ssh_config should contain new port option"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_ssh_config_remove() {
    let config_file = std::path::Path::new("/tmp/test_ssh_config_remove");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "Host github.com\n    hostname github.com\n    user git\n\nHost gitlab.com\n    hostname gitlab.com\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test ssh_config module remove entry
  ssh_config:
    host: github.com
    state: absent
    ssh_config_file: /tmp/test_ssh_config_remove
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
        !content.contains("Host github.com"),
        "ssh_config should not contain github.com entry"
    );
    assert!(
        content.contains("Host gitlab.com"),
        "ssh_config should still contain gitlab.com entry"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_ssh_config_wildcard_pattern() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test ssh_config module with wildcard
  ssh_config:
    host: "*.example.com"
    options:
      user: deploy
      port: 2222
    ssh_config_file: /tmp/test_ssh_config_wildcard
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

    let config_file = std::path::Path::new("/tmp/test_ssh_config_wildcard");
    let content = std::fs::read_to_string(config_file).unwrap();
    assert!(
        content.contains("Host *.example.com"),
        "ssh_config should contain wildcard pattern"
    );
    assert!(
        content.contains("port 2222"),
        "ssh_config should contain port option"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_ssh_config_order_first() {
    let config_file = std::path::Path::new("/tmp/test_ssh_config_order");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "Host existing.com\n    hostname existing.com\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test ssh_config module order first
  ssh_config:
    host: github.com
    options:
      hostname: github.com
    ssh_config_file: /tmp/test_ssh_config_order
    order: first
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
        content.starts_with("Host github.com"),
        "ssh_config should start with github.com entry"
    );

    std::fs::remove_file(config_file).ok();
}
