use crate::cli::modules::run_test;

#[test]
fn test_luks_parse_error_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test luks with invalid field
  luks:
    device: /dev/sdb1
    invalid_field: value
    state: present
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_luks_parse_error_missing_creds() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test luks without creds
  luks:
    device: /dev/sdb1
    state: present
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_luks_parse_error_opened_no_name() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test luks opened without name
  luks:
    device: /dev/sdb1
    passphrase: secret
    state: opened
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_luks_parse_error_closed_no_name() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test luks closed without name
  luks:
    device: /dev/sdb1
    state: closed
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}
