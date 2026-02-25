use crate::cli::modules::run_test;

#[test]
fn test_expect_basic_command() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test expect module with simple command
  expect:
    command: echo hello
    responses: {}
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("hello"));
}

#[test]
fn test_expect_with_creates_file_exists() {
    use std::fs::File;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let marker_file = dir.path().join("marker.txt");
    File::create(&marker_file).unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Test expect with creates - should skip
  expect:
    command: echo "this should not run"
    creates: "{}"
    responses: {{}}
        "#,
        marker_file.to_str().unwrap().replace('\\', "\\\\")
    );

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(!stdout.contains("this should not run"));
}

#[test]
fn test_expect_with_timeout() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Test expect with timeout
  expect:
    command: echo "quick output"
    responses: {}
    timeout: 5
        "#
    .to_string();

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("quick output"));
}

#[test]
fn test_expect_with_chdir() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Test expect with chdir
  expect:
    command: pwd
    chdir: "{}"
    responses: {{}}
        "#,
        dir.path().to_str().unwrap().replace('\\', "\\\\")
    );

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains(dir.path().to_str().unwrap()));
}
