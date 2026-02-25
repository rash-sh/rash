use crate::cli::modules::run_test;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn get_unique_key_path() -> String {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_test_openssl_key_{}.pem", test_id)
}

#[test]
fn test_openssl_privatekey_generate_rsa() {
    let key_path = get_unique_key_path();
    let _ = std::fs::remove_file(&key_path);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Generate RSA private key
  openssl_privatekey:
    path: {}
    size: 2048
    "#,
        key_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&key_path);
}

#[test]
fn test_openssl_privatekey_generate_ecc() {
    let key_path = get_unique_key_path();
    let _ = std::fs::remove_file(&key_path);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Generate ECC private key
  openssl_privatekey:
    path: {}
    type: ecc
    "#,
        key_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&key_path);
}

#[test]
fn test_openssl_privatekey_with_custom_mode() {
    let key_path = get_unique_key_path();
    let _ = std::fs::remove_file(&key_path);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Generate key with custom mode
  openssl_privatekey:
    path: {}
    mode: "0600"
    "#,
        key_path
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&key_path);
}

#[test]
fn test_openssl_privatekey_idempotent() {
    let key_path = get_unique_key_path();
    let _ = std::fs::remove_file(&key_path);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Generate key first time
  openssl_privatekey:
    path: {}
    size: 2048

- name: Try to generate same key again
  openssl_privatekey:
    path: {}
    size: 2048
    "#,
        key_path, key_path
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);

    let _ = std::fs::remove_file(&key_path);
}

#[test]
fn test_openssl_privatekey_force_regenerate() {
    let key_path = get_unique_key_path();
    let _ = std::fs::remove_file(&key_path);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Generate key first time
  openssl_privatekey:
    path: {}
    size: 2048

- name: Force regenerate key
  openssl_privatekey:
    path: {}
    size: 2048
    force: true
    "#,
        key_path, key_path
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);

    let _ = std::fs::remove_file(&key_path);
}

#[test]
fn test_openssl_privatekey_absent() {
    let key_path = get_unique_key_path();
    let _ = std::fs::remove_file(&key_path);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Generate key
  openssl_privatekey:
    path: {}
    size: 2048

- name: Remove key
  openssl_privatekey:
    path: {}
    state: absent

- name: Try to remove non-existent key
  openssl_privatekey:
    path: /tmp/nonexistent_key_{}.pem
    state: absent
    "#,
        key_path,
        key_path,
        TEST_COUNTER.load(Ordering::SeqCst)
    );

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);

    let _ = std::fs::remove_file(&key_path);
}
