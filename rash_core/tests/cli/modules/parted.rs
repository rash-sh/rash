use crate::cli::modules::run_test;

#[test]
fn test_parted_error_missing_number_for_present() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test parted module error when number missing for present state
  parted:
    device: /dev/sdb
    state: present
    "#;
    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("number is required when state=present"));
}

#[test]
fn test_parted_error_missing_number_for_absent() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test parted module error when number missing for absent state
  parted:
    device: /dev/sdb
    state: absent
    "#;
    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("number is required when state=absent"));
}

#[test]
fn test_parted_error_invalid_unit() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test parted module with invalid unit
  parted:
    device: /dev/sdb
    unit: MiB
    "#;
    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown variant"));
}

#[test]
fn test_parted_error_invalid_state() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test parted module with invalid state
  parted:
    device: /dev/sdb
    state: invalid
    "#;
    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown variant"));
}

#[test]
fn test_parted_error_invalid_align() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test parted module with invalid align
  parted:
    device: /dev/sdb
    align: invalid
    "#;
    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown variant"));
}

#[test]
fn test_parted_error_invalid_label() {
    let script_text = r#"
#!/usr/bin/env rash
- name: test parted module with invalid label
  parted:
    device: /dev/sdb
    label: invalid
    "#;
    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("unknown variant"));
}
