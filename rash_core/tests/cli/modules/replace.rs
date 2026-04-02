use super::*;

#[test]
fn test_replace_simple() {
    let (stdout, stderr) = run_test(
        r#"
- replace:
    path: /tmp/test.txt
    regexp: 'hello'
    replace: 'hi'
"#,
        &[],
    );
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}

#[test]
fn test_replace_with_backup() {
    let (stdout, stderr) = run_test(
        r#"
- replace:
    path: /tmp/test.txt
    regexp: 'old'
    replace: 'new'
    backup: true
"#,
        &[],
    );
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}

#[test]
fn test_replace_with_after_before() {
    let (stdout, stderr) = run_test(
        r#"
- replace:
    path: /tmp/test.txt
    regexp: 'content'
    replace: 'modified'
    after: 'START'
    before: 'END'
"#,
        &[],
    );
    assert!(stderr.contains("changed") || stdout.contains("changed"));
}
