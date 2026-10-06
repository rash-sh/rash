use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn test_pause_hidden_input_is_registered_but_never_logged() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("script.rh");
    std::fs::write(
        &script,
        r#"
- pause:
    prompt: "Password: "
    input: true
    echo: false
  register: password
- assert:
    that:
      - password.output == "typed-secret"
- debug:
    msg: hidden-input-ok
"#,
    )
    .unwrap();
    for output in ["ansible", "json"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rash"))
            .args(["--output", output])
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"typed-secret\n")
            .unwrap();
        let result = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);

        assert!(stdout.contains("hidden-input-ok"), "stderr: {stderr}");
        assert!(!stdout.contains("typed-secret"), "stdout: {stdout}");
        assert!(!stderr.contains("typed-secret"), "stderr: {stderr}");
    }
}
