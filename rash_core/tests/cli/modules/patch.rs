use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_patch_apply() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let patch_path = dir.path().join("test.patch");

    fs::write(&file_path, "hello world\n").unwrap();
    fs::write(
        &patch_path,
        "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
- patch:
    src: {}
    dest: {}
"#,
        patch_path.display(),
        file_path.display()
    );
    let (stdout, stderr) = run_test(&script_text, &[]);
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}

#[test]
fn test_patch_with_backup() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let patch_path = dir.path().join("test.patch");

    fs::write(&file_path, "hello world\n").unwrap();
    fs::write(
        &patch_path,
        "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
- patch:
    src: {}
    dest: {}
    backup: true
"#,
        patch_path.display(),
        file_path.display()
    );
    let (stdout, stderr) = run_test(&script_text, &[]);
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}

#[test]
fn test_patch_with_strip() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let patch_path = dir.path().join("test.patch");

    fs::write(&file_path, "hello world\n").unwrap();
    fs::write(
        &patch_path,
        "--- a/test.txt\n+++ b/test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
    )
    .unwrap();

    let script_text = format!(
        r#"
- patch:
    src: {}
    dest: {}
    strip: 1
"#,
        patch_path.display(),
        file_path.display()
    );
    let (stdout, stderr) = run_test(&script_text, &[]);
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}
