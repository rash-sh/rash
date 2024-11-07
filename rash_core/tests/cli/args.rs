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
