use super::execute_rash;

#[test]
fn test_environment_variables() {
    let script_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/cli/environment.rh");
    let (stdout, stderr) = execute_rash(&[script_path]);

    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
    assert!(!stdout.is_empty(), "stdout should not be empty");
}
