use crate::cli::modules::run_test;

#[test]
fn test_locale_set_lang() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set LANG
  locale:
    lang: en_US.UTF-8
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.contains("ok:"));
}

#[test]
fn test_locale_set_multiple_vars() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set multiple locale variables
  locale:
    lang: en_US.UTF-8
    lc_all: en_US.UTF-8
    lc_ctype: en_US.UTF-8
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:") || stdout.contains("ok:"));
}

#[test]
fn test_locale_invalid_field() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Set invalid locale field
  locale:
    invalid: value
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
}

#[test]
fn test_locale_no_params() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Locale with no params
  locale: {}
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("ok:"));
}
