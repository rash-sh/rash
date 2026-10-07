use nix::sys::termios::{LocalFlags, tcgetattr};

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Generous deadline: rash can be slow to start on a loaded machine.
const LIMIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);

const HIDDEN_INPUT_SCRIPT: &str = r#"
- block:
    - pause:
        prompt: "Password: "
        input: true
        echo: false
      register: password
    - debug:
        msg: not-reached
  always:
    - debug:
        msg: always-ran
"#;

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

/// Run rash as session leader of a new pseudo-terminal, which `echo: false` needs to open
/// `/dev/tty`. Returns the child, the master and a slave descriptor to inspect the terminal.
fn spawn_on_pty(script: &Path) -> (Child, File, File) {
    let pty = nix::pty::openpty(None, None).unwrap();
    let slave = File::from(pty.slave);
    let mut command = Command::new(env!("CARGO_BIN_EXE_rash"));
    command
        .arg(script)
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave.try_clone().unwrap());
    // SAFETY: only async-signal-safe calls between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().unwrap();
    (child, File::from(pty.master), slave)
}

fn echo_enabled(slave: &File) -> bool {
    tcgetattr(slave)
        .unwrap()
        .local_flags
        .contains(LocalFlags::ECHO)
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < LIMIT, "timed out waiting for {what}");
        thread::sleep(POLL);
    }
}

/// Start rash on a PTY, wait until the hidden prompt turned echo off, let `interrupt` stop
/// it and return its exit code, whether echo is back on and everything it printed.
fn interrupt_hidden_prompt(interrupt: impl FnOnce(&Child, &mut File)) -> (i32, bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("script.rh");
    std::fs::write(&script, HIDDEN_INPUT_SCRIPT).unwrap();
    let (mut child, mut master, slave) = spawn_on_pty(&script);
    let mut reader = master.try_clone().unwrap();
    let drain = thread::spawn(move || {
        let mut output = Vec::new();
        // EIO once every slave descriptor is closed.
        let _ = reader.read_to_end(&mut output);
        String::from_utf8_lossy(&output).to_string()
    });

    wait_until("echo off", || !echo_enabled(&slave));
    interrupt(&child, &mut master);

    let mut status = None;
    wait_until("rash to exit", || {
        status = child.try_wait().unwrap();
        status.is_some()
    });
    let echo_restored = echo_enabled(&slave);
    drop(slave);
    drop(master);
    let output = drain.join().unwrap();
    dbg!(&output);
    (status.unwrap().code().unwrap(), echo_restored, output)
}

#[test]
fn test_pause_hidden_input_ctrl_c_restores_echo_and_interrupts() {
    let (code, echo_restored, output) =
        interrupt_hidden_prompt(|_, master| master.write_all(b"\x03").unwrap());

    assert_eq!(code, 128 + libc::SIGINT, "{output}");
    assert!(echo_restored, "terminal left without echo");
    assert!(output.contains("interrupted by signal"));
    assert!(output.contains("always-ran"));
    assert!(!output.contains("not-reached"));
}

#[test]
fn test_pause_hidden_input_sigterm_restores_echo_and_interrupts() {
    let (code, echo_restored, output) = interrupt_hidden_prompt(|child, _| {
        // SAFETY: plain kill(2) to a child this test spawned and has not reaped yet.
        assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    });

    assert_eq!(code, 128 + libc::SIGTERM, "{output}");
    assert!(echo_restored, "terminal left without echo");
    assert!(output.contains("always-ran"));
    assert!(!output.contains("not-reached"));
}

#[test]
fn test_pause_hidden_input_ctrl_d_is_empty_and_restores_echo() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("script.rh");
    std::fs::write(
        &script,
        r#"
- pause:
    input: true
    echo: false
  register: password
- assert:
    that:
      - password.output == ""
- debug:
    msg: eof-ok
"#,
    )
    .unwrap();
    let (mut child, mut master, slave) = spawn_on_pty(&script);
    let mut reader = master.try_clone().unwrap();
    let drain = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = reader.read_to_end(&mut output);
        String::from_utf8_lossy(&output).to_string()
    });

    wait_until("echo off", || !echo_enabled(&slave));
    master.write_all(b"\x04").unwrap();
    let mut status = None;
    wait_until("rash to exit", || {
        status = child.try_wait().unwrap();
        status.is_some()
    });
    let echo_restored = echo_enabled(&slave);
    drop(slave);
    drop(master);
    let output = drain.join().unwrap();

    assert!(status.unwrap().success(), "{output}");
    assert!(echo_restored, "terminal left without echo");
    assert!(output.contains("eof-ok"));
}
