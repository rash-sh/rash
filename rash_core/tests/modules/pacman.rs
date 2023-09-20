use std::env;
use std::fs::File;
use std::io::Write;
use std::iter;
use std::path::Path;
use std::process::Command;

use serde_json::json;
use tempfile::tempdir;

fn update_path(new_path: &Path) {
    let path = env::var_os("PATH").unwrap();
    let paths = iter::once(new_path.to_path_buf())
        .chain(env::split_paths(&path))
        .collect::<Vec<_>>();
    let new_path = env::join_paths(paths).unwrap();
    env::set_var("PATH", new_path);
}

#[test]
fn test_pacman_present() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    force: true
    name:
      - rustup
      - bpftrace
      - linux61-zfs
    state: present
        "#,
        mocks_dir.to_str().unwrap()
    );

    let tmp_dir = tempdir().unwrap();
    let script_path = tmp_dir.path().join("script.rh");
    let mut script_file = File::create(&script_path).unwrap();
    script_file.write_all(script_text.as_bytes()).unwrap();

    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    update_path(bin_path.parent().unwrap());

    let output = Command::new(bin_path)
        .arg("--diff")
        .arg(script_path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines();

    dbg!(&stdout);
    dbg!(String::from_utf8_lossy(&output.stderr));

    assert!(lines.clone().any(|x| x == "+ rustup"));
    assert!(lines.clone().any(|x| x == "+ bpftrace"));
    assert!(lines.clone().all(|x| x != "+ linux61-zfs"));
    assert!(output.stderr.is_empty());
    assert!(lines
        .clone()
        .last()
        .unwrap()
        .starts_with("\u{1b}[33mchanged"));
}

#[test]
fn test_pacman_remove() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    force: true
    name:
      - linux61-nvidia
      - linux61-zfs
      - rash
    state: absent
        "#,
        mocks_dir.to_str().unwrap()
    );

    let tmp_dir = tempdir().unwrap();
    let script_path = tmp_dir.path().join("script.rh");
    let mut script_file = File::create(&script_path).unwrap();
    script_file.write_all(script_text.as_bytes()).unwrap();

    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    update_path(bin_path.parent().unwrap());

    let output = Command::new(bin_path)
        .arg("--diff")
        .arg(script_path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines();

    dbg!(&stdout);
    dbg!(String::from_utf8_lossy(&output.stderr));

    assert!(lines.clone().any(|x| x == "- linux61-nvidia"));
    assert!(lines.clone().any(|x| x == "- linux61-zfs"));
    assert!(lines.clone().all(|x| x != "- rash"));
    assert!(lines
        .clone()
        .last()
        .unwrap()
        .starts_with("\u{1b}[33mchanged"));
    assert!(output.stderr.is_empty());
}

#[test]
fn test_pacman_sync() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    name:
      - linux61-nvidia
      - linux61-zfs
      - rash
    state: sync
        "#,
        mocks_dir.to_str().unwrap()
    );

    let tmp_dir = tempdir().unwrap();
    let script_path = tmp_dir.path().join("script.rh");
    let mut script_file = File::create(&script_path).unwrap();
    script_file.write_all(script_text.as_bytes()).unwrap();

    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    update_path(bin_path.parent().unwrap());

    let output = Command::new(bin_path)
        .arg("--diff")
        .arg(script_path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines();

    dbg!(&stdout);
    dbg!(String::from_utf8_lossy(&output.stderr));

    assert!(lines.clone().any(|x| x == "- linux-firmware"));
    assert!(lines.clone().any(|x| x == "- linux61"));
    assert!(lines.clone().any(|x| x == "+ rash"));
    assert!(lines
        .clone()
        .last()
        .unwrap()
        .starts_with("\u{1b}[33mchanged"));
    assert!(output.stderr.is_empty());
}

#[test]
fn test_pacman_sync_upgrade_no_changed() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    upgrade: true
    name:
      - linux-firmware
      - linux61
      - linux61-nvidia
      - linux61-zfs
    state: sync
        "#,
        mocks_dir.to_str().unwrap()
    );

    let tmp_dir = tempdir().unwrap();
    let script_path = tmp_dir.path().join("script.rh");
    let mut script_file = File::create(&script_path).unwrap();
    script_file.write_all(script_text.as_bytes()).unwrap();

    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    update_path(bin_path.parent().unwrap());

    let output = Command::new(bin_path)
        .arg("--diff")
        .arg(script_path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines();

    dbg!(&stdout);
    dbg!(String::from_utf8_lossy(&output.stderr));

    assert!(lines.clone().all(|x| x != "+ linux-firmware"));
    assert!(lines.clone().all(|x| x != "+ linux61"));
    assert!(lines.clone().all(|x| x != "+ linux61-nvidia"));
    assert!(lines.clone().all(|x| x != "+ linux61-zfs"));
    assert!(lines.clone().last().unwrap().starts_with("\u{1b}[32mok"));
    assert!(output.stderr.is_empty());
}

#[test]
fn test_pacman_result_extra() {
    let mocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: test pacman module
  pacman:
    executable: {}/pacman.rh
    upgrade: true
    name:
      - linux-firmware
      - linux61
      - linux61-nvidia
      - rash
    state: sync
  register: packages
- debug:
    msg: "{{{{ packages.extra | json_encode }}}}"
        "#,
        mocks_dir.to_str().unwrap()
    );

    let tmp_dir = tempdir().unwrap();
    let script_path = tmp_dir.path().join("script.rh");
    let mut script_file = File::create(&script_path).unwrap();
    script_file.write_all(script_text.as_bytes()).unwrap();

    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    update_path(bin_path.parent().unwrap());

    let output = Command::new(bin_path)
        .args(["--output", "raw"])
        .arg(script_path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines();

    dbg!(&stdout);
    dbg!(String::from_utf8_lossy(&output.stderr));

    assert!(output.stderr.is_empty());
    assert_eq!(
        lines.clone().last().unwrap(),
        serde_json::to_string(&json!({
            "installed_packages": ["rash"],
            "removed_packages": ["linux61-zfs"],
            "upgraded": false,
        }))
        .unwrap()
    );
}
