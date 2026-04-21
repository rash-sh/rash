use crate::cli::modules::run_test;

#[test]
fn test_haproxy_create_backend() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test haproxy module create backend
  haproxy:
    config_file: /tmp/test_haproxy_create.cfg
    name: web_backend
    state: present
    balance: roundrobin
    servers:
      - name: web1
        address: 192.168.1.10:80
      - name: web2
        address: 192.168.1.11:80
        check: true
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

    let config_file = std::path::Path::new("/tmp/test_haproxy_create.cfg");
    assert!(config_file.exists(), "haproxy config file should exist");

    let content = std::fs::read_to_string(config_file).unwrap();
    assert!(
        content.contains("backend web_backend"),
        "should contain backend section"
    );
    assert!(
        content.contains("balance roundrobin"),
        "should contain balance"
    );
    assert!(
        content.contains("server web1 192.168.1.10:80"),
        "should contain web1"
    );
    assert!(
        content.contains("server web2 192.168.1.11:80 check"),
        "should contain web2 with check"
    );

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_haproxy_remove_backend() {
    let config_file = std::path::Path::new("/tmp/test_haproxy_remove.cfg");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "global\n    log local0\n\nbackend old_backend\n    balance roundrobin\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test haproxy module remove backend
  haproxy:
    config_file: /tmp/test_haproxy_remove.cfg
    name: old_backend
    state: absent
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
        !content.contains("old_backend"),
        "should not contain removed backend"
    );
    assert!(content.contains("global"), "should preserve global section");

    std::fs::remove_file(config_file).ok();
}

#[test]
fn test_haproxy_no_change() {
    let config_file = std::path::Path::new("/tmp/test_haproxy_nochange.cfg");
    std::fs::create_dir_all(config_file.parent().unwrap()).ok();
    std::fs::write(
        config_file,
        "backend web_backend\n    balance roundrobin\n    server web1 192.168.1.10:80\n",
    )
    .ok();

    let script_text = r#"
#!/usr/bin/env rash
- name: test haproxy module no change
  haproxy:
    config_file: /tmp/test_haproxy_nochange.cfg
    name: web_backend
    state: present
    balance: roundrobin
    servers:
      - name: web1
        address: 192.168.1.10:80
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
