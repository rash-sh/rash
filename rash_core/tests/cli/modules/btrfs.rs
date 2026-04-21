use crate::cli::modules::run_test;

#[test]
fn test_btrfs_parse_error_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test btrfs with invalid field
  btrfs:
    device: /dev/sda1
    subvolume: /data/app
    invalid_field: value
    state: present
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_btrfs_parse_error_missing_device() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test btrfs without device
  btrfs:
    subvolume: /data/app
    state: present
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_btrfs_parse_error_missing_subvolume() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test btrfs without subvolume
  btrfs:
    device: /dev/sda1
    state: present
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_btrfs_parse_error_invalid_state() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test btrfs with invalid state
  btrfs:
    device: /dev/sda1
    subvolume: /data/app
    state: invalid
    "#;

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(script_text, &args);

    assert!(!stderr.is_empty());
}
