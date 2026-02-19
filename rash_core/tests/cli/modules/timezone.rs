use crate::cli::modules::run_test;

#[test]
fn test_timezone_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set timezone in check mode
  timezone:
    name: Europe/Madrid
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_timezone_utc() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set timezone to UTC
  timezone:
    name: UTC
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.contains("ok:"));
}

#[test]
fn test_timezone_invalid_empty() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set empty timezone
  timezone:
    name: ""
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_timezone_invalid_timezone() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set invalid timezone
  timezone:
    name: Invalid/Timezone
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("not found"));
}

#[test]
fn test_timezone_america() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set timezone to America/New_York
  timezone:
    name: America/New_York
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}
