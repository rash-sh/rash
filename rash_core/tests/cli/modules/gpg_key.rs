use crate::cli::modules::run_test;

#[test]
fn test_gpg_key_missing_required_params() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test gpg_key module missing params
  gpg_key:
    state: present
"#
    .to_string();

    let args: [&str; 0] = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(
        !stderr.is_empty(),
        "stderr should contain error, got: {}",
        stderr
    );
    assert!(
        stderr.contains("Either key_id or keyfile must be specified"),
        "stderr should contain error message, got: {}",
        stderr
    );
}

#[test]
fn test_gpg_key_both_key_id_and_keyfile() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test gpg_key module with both params
  gpg_key:
    key_id: "0x1234567890ABCDEF"
    keyfile: /path/to/key.asc
    keyserver: keyserver.ubuntu.com
    state: present
"#
    .to_string();

    let args: [&str; 0] = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(
        !stderr.is_empty(),
        "stderr should contain error, got: {}",
        stderr
    );
    assert!(
        stderr.contains("Only one of key_id or keyfile can be specified"),
        "stderr should contain error message, got: {}",
        stderr
    );
}

#[test]
fn test_gpg_key_absent_requires_key_id() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test gpg_key module absent with keyfile
  gpg_key:
    keyfile: /path/to/key.asc
    state: absent
"#
    .to_string();

    let args: [&str; 0] = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(
        !stderr.is_empty(),
        "stderr should contain error, got: {}",
        stderr
    );
    assert!(
        stderr.contains("key_id is required when state=absent"),
        "stderr should contain error message, got: {}",
        stderr
    );
}

#[test]
fn test_gpg_key_present_key_id_requires_keyserver() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test gpg_key module present without keyserver
  gpg_key:
    key_id: "0x1234567890ABCDEF"
    state: present
"#
    .to_string();

    let args: [&str; 0] = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(
        !stderr.is_empty(),
        "stderr should contain error, got: {}",
        stderr
    );
    assert!(
        stderr.contains("keyserver is required when using key_id to import a key"),
        "stderr should contain error message, got: {}",
        stderr
    );
}

#[test]
fn test_gpg_key_invalid_state() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test gpg_key module invalid state
  gpg_key:
    key_id: "0x1234567890ABCDEF"
    state: invalid_state
"#
    .to_string();

    let args: [&str; 0] = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(
        !stderr.is_empty(),
        "stderr should contain error for invalid state, got: {}",
        stderr
    );
}

#[test]
fn test_gpg_key_invalid_trust() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test gpg_key module invalid trust
  gpg_key:
    key_id: "0x1234567890ABCDEF"
    keyserver: keyserver.ubuntu.com
    trust: invalid_trust
    state: present
"#
    .to_string();

    let args: [&str; 0] = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(
        !stderr.is_empty(),
        "stderr should contain error for invalid trust, got: {}",
        stderr
    );
}

#[test]
fn test_gpg_key_invalid_type() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test gpg_key module invalid type
  gpg_key:
    key_id: "0x1234567890ABCDEF"
    keyserver: keyserver.ubuntu.com
    type: invalid_type
    state: present
"#
    .to_string();

    let args: [&str; 0] = [];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(
        !stderr.is_empty(),
        "stderr should contain error for invalid type, got: {}",
        stderr
    );
}
