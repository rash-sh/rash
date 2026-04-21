use crate::cli::modules::run_test;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_sysfs_set_value() {
    let dir = tempdir().unwrap();
    let attr_file = dir.path().join("mtu");
    fs::write(&attr_file, "1500").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set MTU
  sysfs:
    path: {}
    value: "9000"
        "#,
        attr_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&attr_file).unwrap();
    assert_eq!(content, "9000");
}

#[test]
fn test_sysfs_idempotent() {
    let dir = tempdir().unwrap();
    let attr_file = dir.path().join("mtu");
    fs::write(&attr_file, "9000").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set MTU first time
  sysfs:
    path: {}
    value: "9000"
        "#,
        attr_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_sysfs_check_mode() {
    let dir = tempdir().unwrap();
    let attr_file = dir.path().join("mtu");
    fs::write(&attr_file, "1500").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Set MTU in check mode
  sysfs:
    path: {}
    value: "9000"
        "#,
        attr_file.display()
    );

    let args = ["--check", "--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&attr_file).unwrap();
    assert_eq!(content, "1500");
}

#[test]
fn test_sysfs_absent_with_value() {
    let dir = tempdir().unwrap();
    let attr_file = dir.path().join("direction");
    fs::write(&attr_file, "out").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove direction value
  sysfs:
    path: {}
    value: "out"
    state: absent
        "#,
        attr_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}
