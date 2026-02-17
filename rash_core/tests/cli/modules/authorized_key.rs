use crate::cli::modules::run_test_with_env;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn get_unique_passwd_file() -> String {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/rash_test_passwd_authorized_{}", test_id)
}

fn setup_passwd_file(passwd_file: &str, home_dir: &str) {
    let _ = std::fs::remove_file(passwd_file);
    std::fs::write(
        passwd_file,
        format!("deploy:x:1000:1000:Deploy User:{}:/bin/bash\n", home_dir),
    )
    .expect("Failed to create test passwd file");
}

#[test]
fn test_authorized_key_add() {
    let passwd_file = get_unique_passwd_file();
    let tmp_dir = tempfile::tempdir().unwrap();
    setup_passwd_file(&passwd_file, tmp_dir.path().to_str().unwrap());

    let script_text = r#"
#!/usr/bin/env rash
- name: test authorized_key module add key
  authorized_key:
    user: deploy
    key: ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCTest... test@host
    state: present
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let keys_file = tmp_dir.path().join(".ssh/authorized_keys");
    assert!(keys_file.exists(), "authorized_keys file should exist");

    let content = std::fs::read_to_string(&keys_file).unwrap();
    assert!(
        content.contains("ssh-rsa"),
        "authorized_keys should contain ssh-rsa key"
    );
    assert!(
        content.contains("test@host"),
        "authorized_keys should contain comment"
    );

    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_authorized_key_add_existing_no_change() {
    let passwd_file = get_unique_passwd_file();
    let tmp_dir = tempfile::tempdir().unwrap();
    setup_passwd_file(&passwd_file, tmp_dir.path().to_str().unwrap());

    let keys_dir = tmp_dir.path().join(".ssh");
    let keys_file = keys_dir.join("authorized_keys");
    std::fs::create_dir_all(&keys_dir).unwrap();
    std::fs::write(
        &keys_file,
        "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCTest... test@host\n",
    )
    .unwrap();

    let script_text = r#"
#!/usr/bin/env rash
- name: test authorized_key module add existing key
  authorized_key:
    user: deploy
    key: ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCTest... test@host
    state: present
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok' (no change), got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_authorized_key_remove() {
    let passwd_file = get_unique_passwd_file();
    let tmp_dir = tempfile::tempdir().unwrap();
    setup_passwd_file(&passwd_file, tmp_dir.path().to_str().unwrap());

    let keys_dir = tmp_dir.path().join(".ssh");
    let keys_file = keys_dir.join("authorized_keys");
    std::fs::create_dir_all(&keys_dir).unwrap();
    std::fs::write(
        &keys_file,
        "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCTest... test@host\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... other@host\n",
    )
    .unwrap();

    let script_text = r#"
#!/usr/bin/env rash
- name: test authorized_key module remove key
  authorized_key:
    user: deploy
    key: ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCTest... test@host
    state: absent
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let content = std::fs::read_to_string(&keys_file).unwrap();
    assert!(
        !content.contains("ssh-rsa"),
        "authorized_keys should not contain ssh-rsa key"
    );
    assert!(
        content.contains("ssh-ed25519"),
        "authorized_keys should still contain ssh-ed25519 key"
    );

    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_authorized_key_exclusive() {
    let passwd_file = get_unique_passwd_file();
    let tmp_dir = tempfile::tempdir().unwrap();
    setup_passwd_file(&passwd_file, tmp_dir.path().to_str().unwrap());

    let keys_dir = tmp_dir.path().join(".ssh");
    let keys_file = keys_dir.join("authorized_keys");
    std::fs::create_dir_all(&keys_dir).unwrap();
    std::fs::write(
        &keys_file,
        "ssh-rsa OLDKEY... old@host\nssh-ed25519 OLDKEY2... old2@host\n",
    )
    .unwrap();

    let script_text = r#"
#!/usr/bin/env rash
- name: test authorized_key module exclusive
  authorized_key:
    user: deploy
    key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... new@host
    state: present
    exclusive: true
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let content = std::fs::read_to_string(&keys_file).unwrap();
    assert!(
        content.contains("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI..."),
        "authorized_keys should contain new key"
    );
    assert!(
        !content.contains("OLDKEY"),
        "authorized_keys should not contain old keys"
    );

    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_authorized_key_with_options() {
    let passwd_file = get_unique_passwd_file();
    let tmp_dir = tempfile::tempdir().unwrap();
    setup_passwd_file(&passwd_file, tmp_dir.path().to_str().unwrap());

    let script_text = r#"
#!/usr/bin/env rash
- name: test authorized_key module with options
  authorized_key:
    user: deploy
    key: ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCTest... test@host
    state: present
    key_options: 'no-port-forwarding,from="10.0.1.1"'
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let keys_file = tmp_dir.path().join(".ssh/authorized_keys");
    let content = std::fs::read_to_string(&keys_file).unwrap();
    assert!(
        content.contains("no-port-forwarding"),
        "authorized_keys should contain key options"
    );
    assert!(
        content.contains("from=\"10.0.1.1\""),
        "authorized_keys should contain from option"
    );

    let _ = std::fs::remove_file(&passwd_file);
}

#[test]
fn test_authorized_key_multiple_keys() {
    let passwd_file = get_unique_passwd_file();
    let tmp_dir = tempfile::tempdir().unwrap();
    setup_passwd_file(&passwd_file, tmp_dir.path().to_str().unwrap());

    let script_text = r#"
#!/usr/bin/env rash
- name: test authorized_key module multiple keys
  authorized_key:
    user: deploy
    key:
      - ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCKey1... user1@host
      - ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKey2... user2@host
    state: present
    "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test_with_env(
        &script_text,
        &args,
        &[("RASH_TEST_PASSWD_FILE", &passwd_file)],
    );

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed', got: {}",
        stdout
    );

    let keys_file = tmp_dir.path().join(".ssh/authorized_keys");
    let content = std::fs::read_to_string(&keys_file).unwrap();
    assert!(
        content.contains("ssh-rsa"),
        "authorized_keys should contain ssh-rsa key"
    );
    assert!(
        content.contains("ssh-ed25519"),
        "authorized_keys should contain ssh-ed25519 key"
    );

    let _ = std::fs::remove_file(&passwd_file);
}
