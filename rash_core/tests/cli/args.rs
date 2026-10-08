use super::execute_rash;

#[test]
fn test_script_arg() {
    let script = r#"
    - assert:
        that:
          - rash.path == "{{ rash.dir }}/rash"
    "#;
    let (stdout, _stderr) = execute_rash(&["-s", script]);
    assert!(stdout.contains("ok"));
}

#[test]
fn test_script_arg_and_script_file() {
    let script = r#"
    - assert:
        that:
          - rash.path == "{{ rash.dir }}/script.rh"
    "#;
    let (stdout, _stderr) = execute_rash(&["-s", script, "script.rh"]);
    assert!(stdout.contains("ok"));
}

#[test]
fn test_no_script_arg_and_no_script_file() {
    let (_stdout, stderr) = execute_rash(&[]);
    assert!(stderr.contains("Please provide either <SCRIPT_FILE> or --script."));
}

#[test]
fn test_help_describes_options() {
    let (stdout, _stderr) = execute_rash(&["--help"]);
    for description in [
        "run operations with become",
        "Privilege escalation method to use",
        "Execute in dry-run mode without modifications",
        "Set environment variables",
        "Inline script to be executed",
        "Path to the script file to be executed",
    ] {
        assert!(stdout.contains(description), "missing {description:?}");
    }
    assert!(!stdout.contains("internal-task"));
}

#[test]
fn test_parse_error_of_the_script_form_is_reported() {
    let sequence = r#"
    - command: echo hi
      become_method: doas
    "#;
    let (_stdout, stderr) = execute_rash(&["-s", sequence]);
    assert!(stderr.contains("Invalid become_method 'doas'"), "{stderr}");

    let mapping = r#"
    vars: {}
    tasks:
      - command: echo hi
    "#;
    let (_stdout, stderr) = execute_rash(&["-s", mapping]);
    assert!(
        stderr.contains("Unknown top-level script key: \"vars\""),
        "{stderr}"
    );
}
