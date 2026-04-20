use crate::cli::modules::run_test;
use base64::Engine;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_htpasswd_present_sha256() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add user with SHA-256
  htpasswd:
    path: {}
    name: admin
    password: secret123
    crypt: sha256
    state: present
        "#,
        htpasswd_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&htpasswd_file).unwrap();
    assert!(content.contains("admin:{SHA256}"));
}

#[test]
fn test_htpasswd_present_apr1() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add user with apr1
  htpasswd:
    path: {}
    name: admin
    password: secret123
    state: present
        "#,
        htpasswd_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&htpasswd_file).unwrap();
    assert!(content.contains("admin:$apr1$"));
}

#[test]
fn test_htpasswd_present_sha512() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add user with SHA-512
  htpasswd:
    path: {}
    name: admin
    password: secret123
    crypt: sha512
    state: present
        "#,
        htpasswd_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&htpasswd_file).unwrap();
    assert!(content.contains("admin:{SHA512}"));
}

#[test]
fn test_htpasswd_idempotent() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add user first time
  htpasswd:
    path: {}
    name: admin
    password: secret123
    crypt: sha256
    state: present

- name: Add same user second time
  htpasswd:
    path: {}
    name: admin
    password: secret123
    crypt: sha256
    state: present
        "#,
        htpasswd_file.display(),
        htpasswd_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&htpasswd_file).unwrap();
    assert_eq!(content.lines().count(), 1);
}

#[test]
fn test_htpasswd_absent() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");
    fs::write(
        &htpasswd_file,
        "admin:{SHA256}abc123\nuser2:{SHA256}def456\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Remove admin user
  htpasswd:
    path: {}
    name: admin
    state: absent
        "#,
        htpasswd_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&htpasswd_file).unwrap();
    assert!(!content.contains("admin:"));
    assert!(content.contains("user2:"));
}

#[test]
fn test_htpasswd_check_mode() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add user in check mode
  htpasswd:
    path: {}
    name: admin
    password: secret123
    crypt: sha256
    state: present
        "#,
        htpasswd_file.display()
    );

    let args = ["--check", "--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(!htpasswd_file.exists());
}

#[test]
fn test_htpasswd_preserves_other_users() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"otherpass");
    let other_hash = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
    fs::write(
        &htpasswd_file,
        format!("otheruser:{{SHA256}}{other_hash}\n"),
    )
    .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add admin user
  htpasswd:
    path: {}
    name: admin
    password: secret123
    crypt: sha256
    state: present
        "#,
        htpasswd_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&htpasswd_file).unwrap();
    assert!(content.contains("otheruser:"));
    assert!(content.contains("admin:"));
}

#[test]
fn test_htpasswd_update_password() {
    let dir = tempdir().unwrap();
    let htpasswd_file = dir.path().join(".htpasswd");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Add user
  htpasswd:
    path: {}
    name: admin
    password: oldpass
    crypt: sha256
    state: present

- name: Update user password
  htpasswd:
    path: {}
    name: admin
    password: newpass
    crypt: sha256
    state: present
        "#,
        htpasswd_file.display(),
        htpasswd_file.display()
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    let content = fs::read_to_string(&htpasswd_file).unwrap();
    assert_eq!(content.lines().count(), 1);
}
