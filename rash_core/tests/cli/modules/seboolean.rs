use crate::cli::modules::run_test;

#[test]
fn test_seboolean_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Enable HTTPD network connect
  seboolean:
    name: httpd_can_network_connect
    state: true
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("getsebool")
            || stderr.contains("getsebool: command not found")
    );
    assert!(stdout.contains("httpd_can_network_connect") || !stderr.is_empty());
}

#[test]
fn test_seboolean_with_persistent() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Enable persistent HTTPD network connect
  seboolean:
    name: httpd_can_network_connect
    state: true
    persistent: true
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("getsebool")
            || stderr.contains("getsebool: command not found")
    );
    assert!(stdout.contains("httpd_can_network_connect") || !stderr.is_empty());
}

#[test]
fn test_seboolean_state_false() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Disable FTP home directory
  seboolean:
    name: ftp_home_dir
    state: false
        "#
    .to_string();

    let args = ["--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.is_empty()
            || stderr.contains("getsebool")
            || stderr.contains("getsebool: command not found")
    );
    assert!(stdout.contains("ftp_home_dir") || !stderr.is_empty());
}

#[test]
fn test_seboolean_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Invalid seboolean call
  seboolean:
    name: httpd_can_network_connect
    state: true
    invalid_field: value
        "#
    .to_string();

    let args = ["--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown field") || stderr.contains("invalid"));
}
