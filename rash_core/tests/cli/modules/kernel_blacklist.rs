use crate::cli::modules::run_test;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_kernel_blacklist_present() {
    let dir = tempdir().unwrap();
    let blacklist_file = dir.path().join("blacklist.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Blacklist nouveau module
  kernel_blacklist:
    name: nouveau
    state: present
    blacklist_file: {}
        "#,
        blacklist_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&blacklist_file).unwrap();
    assert!(content.contains("blacklist nouveau"));
}

#[test]
fn test_kernel_blacklist_idempotent() {
    let dir = tempdir().unwrap();
    let blacklist_file = dir.path().join("blacklist.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Blacklist nouveau first time
  kernel_blacklist:
    name: nouveau
    state: present
    blacklist_file: {}

- name: Blacklist nouveau second time
  kernel_blacklist:
    name: nouveau
    state: present
    blacklist_file: {}
        "#,
        blacklist_file.display(),
        blacklist_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
}

#[test]
fn test_kernel_blacklist_absent() {
    let dir = tempdir().unwrap();
    let blacklist_file = dir.path().join("blacklist.conf");
    fs::write(&blacklist_file, "blacklist nouveau\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove nouveau from blacklist
  kernel_blacklist:
    name: nouveau
    state: absent
    blacklist_file: {}
        "#,
        blacklist_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&blacklist_file).unwrap();
    assert!(!content.contains("blacklist nouveau"));
}

#[test]
fn test_kernel_blacklist_check_mode() {
    let dir = tempdir().unwrap();
    let blacklist_file = dir.path().join("blacklist.conf");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Blacklist module in check mode
  kernel_blacklist:
    name: nouveau
    state: present
    blacklist_file: {}
        "#,
        blacklist_file.display()
    );

    let args = ["--check", "--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(!blacklist_file.exists());
}

#[test]
fn test_kernel_blacklist_preserves_other_entries() {
    let dir = tempdir().unwrap();
    let blacklist_file = dir.path().join("blacklist.conf");
    fs::write(&blacklist_file, "blacklist nvidia\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Blacklist nouveau
  kernel_blacklist:
    name: nouveau
    state: present
    blacklist_file: {}
        "#,
        blacklist_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&blacklist_file).unwrap();
    assert!(content.contains("blacklist nvidia"));
    assert!(content.contains("blacklist nouveau"));
}
