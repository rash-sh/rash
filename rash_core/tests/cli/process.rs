//! Process execution semantics: stdio modes, signals and terminal handling.
use crate::cli::modules::run_test;

use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::{TempDir, tempdir};

const LIMIT: Duration = Duration::from_secs(10);

#[test]
fn test_stdio_tee_inherit_and_unread_stdin() {
    let script_text = r#"
#!/usr/bin/env rash
- command:
    cmd: printf tee-marker
    stdout: tee
  register: teed
- command:
    cmd: printf inherit-marker
    stdout: inherit
  register: inherited
- command:
    argv: [head, -c1]
    stdin: "{{ 'x' * 1000000 }}"
  register: head
- assert:
    that:
      - teed.stdout == "tee-marker"
      - not inherited.stdout
      - head.rc == 0
      - head.stdout == "x"
- debug:
    msg: stdio-ok
        "#;

    let args: &[&str] = &[];
    let (stdout, stderr) = run_test(script_text, args);

    assert!(stdout.contains("stdio-ok"), "stderr: {stderr}");
    // Streamed by the child itself (inherit) or by Rash while capturing (tee).
    assert!(stdout.contains("tee-marker"));
    assert!(stdout.contains("inherit-marker"));
}

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new(script: &str) -> Self {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("script.rh"), script).unwrap();
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rash"));
        command
            .arg(self.path("script.rh"))
            .env("RASH_TEST_MARKER", self.path("marker"))
            .env("RASH_TEST_PIDFILE", self.path("pid"));
        command
    }

    fn spawn(&self) -> Child {
        self.command()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn wait_for(&self, name: &str) -> String {
        let path = self.path(name);
        let start = Instant::now();
        loop {
            if let Ok(content) = fs::read_to_string(&path)
                && (name == "marker" || content.ends_with('\n'))
            {
                return content;
            }
            assert!(start.elapsed() < LIMIT, "{path:?} never appeared");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn async_job_pid(&self) -> i32 {
        self.wait_for("pid").trim().parse().unwrap()
    }
}

fn wait_with_limit(child: &mut Child) -> ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            panic!("rash did not exit within {LIMIT:?}");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn finish(mut child: Child) -> (ExitStatus, String) {
    let status = wait_with_limit(&mut child);
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    dbg!(&stdout, &stderr);
    (status, stdout + &stderr)
}

fn send_signal(child: &Child, signal: i32) {
    // SAFETY: plain kill(2) to a child this test spawned and has not reaped yet.
    assert_eq!(unsafe { libc::kill(child.id() as i32, signal) }, 0);
}

fn process_gone(pid: i32) -> bool {
    let start = Instant::now();
    while start.elapsed() < LIMIT {
        // A zombie is already dead: it only waits for its new parent to reap it.
        let alive = fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| stat.rsplit_once(") ").map(|(_, s)| !s.starts_with('Z')))
            .unwrap_or(false);
        if !alive {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

const ASYNC_JOB: &str = r#"
- command:
    argv: [sh, -c, 'echo $$ > "$RASH_TEST_PIDFILE"; exec sleep 30']
  async: 60
  poll: 0
"#;

const BLOCK_WITH_SLEEPING_CHILD: &str = r#"
- block:
    - name: long running child
      command:
        argv: [sh, -c, 'touch "$RASH_TEST_MARKER"; exec sleep 30']
      ignore_errors: true
      failed_when: false
    - debug:
        msg: after-ignored
  rescue:
    - debug:
        msg: rescue-ran
  always:
    - debug:
        msg: always-ran
- debug:
    msg: after-block
"#;

fn assert_signal_stops_sync_child(signal: i32) {
    let fixture = Fixture::new(&format!("{ASYNC_JOB}{BLOCK_WITH_SLEEPING_CHILD}"));
    let child = fixture.spawn();
    let job_pid = fixture.async_job_pid();
    fixture.wait_for("marker");
    send_signal(&child, signal);

    let (status, output) = finish(child);

    assert_eq!(status.code(), Some(128 + signal), "{output}");
    assert!(output.contains("always-ran"));
    assert!(output.contains("interrupted by signal"));
    assert!(!output.contains("rescue-ran"));
    assert!(!output.contains("after-ignored"));
    assert!(!output.contains("after-block"));
    assert!(process_gone(job_pid), "async job {job_pid} survived");
}

#[test]
fn test_sigterm_interrupts_sync_child_runs_always_and_kills_async_jobs() {
    assert_signal_stops_sync_child(libc::SIGTERM);
}

#[test]
fn test_sigint_interrupts_sync_child_runs_always_and_kills_async_jobs() {
    assert_signal_stops_sync_child(libc::SIGINT);
}

#[test]
fn test_sigterm_between_tasks_exits_and_kills_async_jobs() {
    let script = format!(
        "{ASYNC_JOB}{}",
        r#"
- command:
    argv: [touch, "{{ env.RASH_TEST_MARKER }}"]
- pause:
    seconds: 30
- debug:
    msg: after-pause
"#
    );
    let fixture = Fixture::new(&script);
    let child = fixture.spawn();
    let job_pid = fixture.async_job_pid();
    fixture.wait_for("marker");
    send_signal(&child, libc::SIGTERM);

    let (status, output) = finish(child);

    assert_eq!(status.code(), Some(143), "{output}");
    assert!(!output.contains("after-pause"));
    assert!(process_gone(job_pid), "async job {job_pid} survived");
}

/// Run rash as session leader of a new pseudo-terminal, like a login shell would, so the
/// test does not depend on the CI runner having a TTY.
fn spawn_on_pty(fixture: &Fixture) -> (Child, fs::File) {
    let pty = nix::pty::openpty(None, None).unwrap();
    let slave = fs::File::from(pty.slave);
    let mut command = fixture.command();
    command
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave);
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
    // Drop the parent's slave copies held by `command` so the master sees EOF on exit.
    drop(command);
    (child, fs::File::from(pty.master))
}

#[test]
fn test_sync_child_can_use_controlling_terminal() {
    let script = r#"
- name: configure and read the terminal
  command:
    argv: [sh, -c, 'stty -echo; read line; stty echo; echo "got:$line" > "$RASH_TEST_MARKER"']
"#;
    let fixture = Fixture::new(script);
    let (mut child, mut master) = spawn_on_pty(&fixture);
    let mut reader = master.try_clone().unwrap();
    let drain = thread::spawn(move || {
        let mut output = Vec::new();
        // EIO once every slave descriptor is closed.
        let _ = reader.read_to_end(&mut output);
        String::from_utf8_lossy(&output).to_string()
    });
    master.write_all(b"hello\n").unwrap();

    // A child in a background process group would be stopped by SIGTTOU/SIGTTIN here.
    let status = wait_with_limit(&mut child);
    drop(master);
    let output = drain.join().unwrap();

    assert!(status.success(), "{output}");
    assert_eq!(fixture.wait_for("marker"), "got:hello\n");
}
