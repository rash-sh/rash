use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_replace_simple() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "hello world\n").unwrap();

    let script_text = format!(
        r#"
- replace:
    path: {}
    regexp: 'hello'
    replace: 'hi'
"#,
        file_path.display()
    );
    let (stdout, stderr) = run_test(&script_text, &[]);
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}

#[test]
fn test_replace_with_backup() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "old content\n").unwrap();

    let script_text = format!(
        r#"
- replace:
    path: {}
    regexp: 'old'
    replace: 'new'
    backup: true
"#,
        file_path.display()
    );
    let (stdout, stderr) = run_test(&script_text, &[]);
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}

#[test]
fn test_replace_with_after_before() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "START content END\n").unwrap();

    let script_text = format!(
        r#"
- replace:
    path: {}
    regexp: 'content'
    replace: 'modified'
    after: 'START'
    before: 'END'
"#,
        file_path.display()
    );
    let (stdout, stderr) = run_test(&script_text, &[]);
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}
