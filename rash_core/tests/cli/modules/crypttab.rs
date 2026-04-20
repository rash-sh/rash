use crate::cli::modules::run_test;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_crypttab_present() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add encrypted swap entry
  crypttab:
    name: cryptswap
    backing_device: /dev/sda2
    password: /dev/urandom
    opts: swap
    state: present
    path: {}
        "#,
        crypttab_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&crypttab_file).unwrap();
    assert!(content.contains("cryptswap /dev/sda2 /dev/urandom swap"));
}

#[test]
fn test_crypttab_present_minimal() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add encrypted entry with minimal params
  crypttab:
    name: cryptdata
    backing_device: /dev/sdb1
    state: present
    path: {}
        "#,
        crypttab_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&crypttab_file).unwrap();
    assert!(content.contains("cryptdata /dev/sdb1 none"));
}

#[test]
fn test_crypttab_idempotent() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add encrypted entry first time
  crypttab:
    name: cryptswap
    backing_device: /dev/sda2
    password: /dev/urandom
    opts: swap
    state: present
    path: {}

- name: Add same entry second time
  crypttab:
    name: cryptswap
    backing_device: /dev/sda2
    password: /dev/urandom
    opts: swap
    state: present
    path: {}
        "#,
        crypttab_file.display(),
        crypttab_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&crypttab_file).unwrap();
    assert_eq!(content.lines().count(), 1);
}

#[test]
fn test_crypttab_absent() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");
    fs::write(&crypttab_file, "cryptswap /dev/sda2 /dev/urandom swap\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove cryptswap entry
  crypttab:
    name: cryptswap
    state: absent
    path: {}
        "#,
        crypttab_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&crypttab_file).unwrap();
    assert!(!content.contains("cryptswap"));
}

#[test]
fn test_crypttab_check_mode() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add encrypted entry in check mode
  crypttab:
    name: cryptswap
    backing_device: /dev/sda2
    password: /dev/urandom
    opts: swap
    state: present
    path: {}
        "#,
        crypttab_file.display()
    );

    let args = ["--check", "--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(!crypttab_file.exists());
}

#[test]
fn test_crypttab_preserves_other_entries() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");
    fs::write(&crypttab_file, "cryptdata /dev/sdb1 none luks\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add cryptswap entry
  crypttab:
    name: cryptswap
    backing_device: /dev/sda2
    password: /dev/urandom
    opts: swap
    state: present
    path: {}
        "#,
        crypttab_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&crypttab_file).unwrap();
    assert!(content.contains("cryptdata"));
    assert!(content.contains("cryptswap"));
}

#[test]
fn test_crypttab_update_existing() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");
    fs::write(&crypttab_file, "cryptswap /dev/sda2 none\n").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Update cryptswap entry
  crypttab:
    name: cryptswap
    backing_device: /dev/sda2
    password: /dev/urandom
    opts: swap
    state: present
    path: {}
        "#,
        crypttab_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&crypttab_file).unwrap();
    assert!(content.contains("/dev/urandom"));
    assert!(content.contains("swap"));
}

#[test]
fn test_crypttab_preserves_comments() {
    let dir = tempdir().unwrap();
    let crypttab_file = dir.path().join("crypttab");
    fs::write(
        &crypttab_file,
        "# This is a comment\ncryptdata /dev/sdb1 none luks\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add cryptswap entry
  crypttab:
    name: cryptswap
    backing_device: /dev/sda2
    password: /dev/urandom
    opts: swap
    state: present
    path: {}
        "#,
        crypttab_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&crypttab_file).unwrap();
    assert!(content.contains("# This is a comment"));
}
