//! Process execution semantics: stdio modes, signals and terminal handling.
use crate::cli::modules::run_test;

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tempfile::{TempDir, tempdir};

/// Generous deadline: rash and its children can be slow to start on a loaded machine.
const LIMIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);

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
    lifeline: File,
}

impl Fixture {
    fn new(script: &str) -> Self {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("script.rh"), script).unwrap();
        // A FIFO async jobs hold open for writing: reading it reports EOF only once every
        // holder exited, which cannot be fooled by pid reuse or unreaped zombies.
        let lifeline = dir.path().join("lifeline");
        nix::unistd::mkfifo(&lifeline, nix::sys::stat::Mode::S_IRWXU).unwrap();
        // Non-blocking: opening does not wait for a writer and reads never block.
        let lifeline = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(lifeline)
            .unwrap();
        Self { dir, lifeline }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn command(&self) -> Command {
        self.command_with_args(&[])
    }

    fn command_with_args(&self, args: &[&std::ffi::OsStr]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rash"));
        command
            .args(args)
            .arg(self.path("script.rh"))
            .env("RASH_TEST_MARKER", self.path("marker"))
            .env("RASH_TEST_LIFELINE", self.path("lifeline"));
        command
    }

    fn spawn(&self) -> Child {
        self.spawn_command(self.command())
    }

    fn spawn_command(&self, mut command: Command) -> Child {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    /// Wait until `name` holds a full line: a file appears empty before it is written.
    fn wait_for(&self, name: &str) -> String {
        let path = self.path(name);
        let start = Instant::now();
        loop {
            if let Ok(content) = fs::read_to_string(&path)
                && content.ends_with('\n')
            {
                return content;
            }
            assert!(start.elapsed() < LIMIT, "{path:?} never appeared");
            thread::sleep(POLL);
        }
    }

    /// Read the lifeline once: `Some(true)` on data, `Some(false)` on EOF, `None` while
    /// a holder keeps it open.
    fn read_lifeline(&mut self, line: &mut Vec<u8>) -> Option<bool> {
        let mut buffer = [0; 64];
        match self.lifeline.read(&mut buffer) {
            Ok(0) => Some(false),
            Ok(n) => {
                line.extend_from_slice(&buffer[..n]);
                Some(true)
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => None,
            Err(e) => panic!("reading lifeline: {e}"),
        }
    }

    /// Wait until an async job holding the lifeline wrote a full line.
    fn wait_async_job_ready(&mut self) {
        let start = Instant::now();
        let mut line = Vec::new();
        // EOF before the job opened the FIFO only means "not yet".
        while !line.ends_with(b"\n") {
            assert!(start.elapsed() < LIMIT, "async job never got ready");
            if self.read_lifeline(&mut line) != Some(true) {
                thread::sleep(POLL);
            }
        }
    }

    /// Wait until every async job holding the lifeline exited.
    fn async_jobs_gone(&mut self) -> bool {
        let start = Instant::now();
        while start.elapsed() < LIMIT {
            match self.read_lifeline(&mut Vec::new()) {
                Some(false) => return true,
                Some(true) => {}
                None => thread::sleep(POLL),
            }
        }
        false
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
        thread::sleep(POLL);
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

const ASYNC_JOB: &str = r#"
- command:
    argv: [sh, -c, 'exec 3>"$RASH_TEST_LIFELINE"; echo ready >&3; exec sleep 60']
  async: 60
  poll: 0
"#;

const BLOCK_WITH_SLEEPING_CHILD: &str = r#"
- block:
    - name: long running child
      command:
        # A builtin writes the marker: with no child left to wait for, the shell cannot
        # outlive a SIGINT (bash keeps going if a waited-for child survived it).
        argv: [sh, -c, 'echo ready > "$RASH_TEST_MARKER"; exec sleep 60']
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
    let mut fixture = Fixture::new(&format!("{ASYNC_JOB}{BLOCK_WITH_SLEEPING_CHILD}"));
    let child = fixture.spawn();
    fixture.wait_async_job_ready();
    fixture.wait_for("marker");
    send_signal(&child, signal);

    let (status, output) = finish(child);

    assert_eq!(status.code(), Some(128 + signal), "{output}");
    assert!(output.contains("always-ran"));
    assert!(output.contains("interrupted by signal"));
    assert!(!output.contains("rescue-ran"));
    assert!(!output.contains("after-ignored"));
    assert!(!output.contains("after-block"));
    assert!(fixture.async_jobs_gone(), "async job survived");
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
- name: wait for signal
  pause:
    seconds: 60
- debug:
    msg: after-pause
"#
    );
    let mut fixture = Fixture::new(&script);
    let mut child = fixture.spawn();
    let stdout = child.stdout.take().unwrap();
    let (lines_tx, lines_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut output = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = lines_tx.send(line.clone());
            output.push_str(&line);
            output.push('\n');
        }
        output
    });
    fixture.wait_async_job_ready();
    // The header is printed once the previous task is done: no child is supervised.
    loop {
        let line = lines_rx
            .recv_timeout(LIMIT)
            .expect("pause task never started");
        if line.contains("wait for signal") {
            break;
        }
    }
    send_signal(&child, libc::SIGTERM);

    let (status, stderr) = finish(child);
    let output = reader.join().unwrap() + &stderr;

    assert_eq!(status.code(), Some(143), "{output}");
    assert!(!output.contains("after-pause"));
    assert!(fixture.async_jobs_gone(), "async job survived");
}

#[test]
fn test_async_job_without_stdin_data_reads_empty_stdin() {
    let script = r#"
- command:
    argv: [sh, -c, 'cat; echo eof']
  async: 600
  poll: 1
  register: job
- assert:
    that:
      - job.stdout == "eof\n"
- debug:
    msg: async-stdin-ok
"#;
    let fixture = Fixture::new(script);
    let mut child = fixture
        .command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Rash's stdin stays open and empty: a job inheriting it would wait until its timeout.
    let stdin = child.stdin.take();

    let (status, output) = finish(child);
    drop(stdin);

    assert!(status.success(), "{output}");
    assert!(output.contains("async-stdin-ok"), "{output}");
}

#[test]
fn test_async_loop_kills_started_jobs_when_an_item_fails_to_start() {
    // The second item starts once the first job is ready, and fails: no such program.
    let script = r#"
- command:
    argv:
      - "{{ item if item == 'sh' else pipe('while [ ! -s \"$RASH_TEST_MARKER\" ]; do sleep 0.01; done; echo /nonexistent/program') }}"
      - -c
      - 'exec 3>"$RASH_TEST_LIFELINE"; echo ready >&3; echo ready > "$RASH_TEST_MARKER"; exec sleep 60'
  loop: [sh, missing]
  async: 60
  poll: 1
- debug:
    msg: after-loop
"#;
    let mut fixture = Fixture::new(script);

    let (status, output) = finish(fixture.spawn());

    assert_eq!(status.code(), Some(1), "{output}");
    assert!(
        output.contains("Failed to execute '/nonexistent/program'"),
        "{output}"
    );
    assert!(!output.contains("after-loop"), "{output}");
    assert_eq!(fixture.wait_for("marker"), "ready\n");
    assert!(fixture.async_jobs_gone(), "job of the first item survived");
}

const BECOME_CHILD_WAITING_FOR_SIGNAL: &str = r#"
- block:
    - command:
        argv: [sh, -c, 'exec 3>"$RASH_TEST_LIFELINE"; echo ready > "$RASH_TEST_MARKER"; exec sleep 60']
      become: true
  always:
    - debug:
        msg: always-ran
- debug:
    msg: after-block
"#;

/// Interrupt rash while a become child runs `sleep`, which holds the lifeline open.
fn assert_signal_stops_become_child(fixture: &mut Fixture, args: &[&std::ffi::OsStr]) {
    let tmp = fixture.path("tmp");
    fs::create_dir(&tmp).unwrap();
    let mut command = fixture.command_with_args(args);
    command.env("TMPDIR", &tmp);
    let child = fixture.spawn_command(command);
    fixture.wait_for("marker");
    send_signal(&child, libc::SIGTERM);

    let (status, output) = finish(child);

    assert_eq!(status.code(), Some(143), "{output}");
    assert!(output.contains("always-ran"), "{output}");
    assert!(output.contains("interrupted by signal 15"), "{output}");
    assert!(!output.contains("after-block"), "{output}");
    let left: Vec<_> = fs::read_dir(&tmp).unwrap().collect();
    assert!(left.is_empty(), "task files left behind: {left:?}");
    assert!(fixture.async_jobs_gone(), "become child process survived");
}

#[test]
fn test_sigterm_during_sudo_become_task_runs_always_and_cleans_up() {
    let mut fixture = Fixture::new(BECOME_CHILD_WAITING_FOR_SIGNAL);
    // Like sudo, run the command after `--` (as the current user).
    let fake_sudo = fixture.path("fake-sudo");
    fs::write(
        &fake_sudo,
        "#!/bin/sh\nwhile [ \"$1\" != -- ]; do shift; done\nshift\nexec \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&fake_sudo, fs::Permissions::from_mode(0o755)).unwrap();
    let args = [
        "--become-method".as_ref(),
        "sudo".as_ref(),
        "--become-exe".as_ref(),
        fake_sudo.as_os_str(),
    ];
    assert_signal_stops_become_child(&mut fixture, &args);
}

/// Switching to another user needs root: skipped otherwise.
#[test]
fn test_sigterm_during_syscall_become_task_runs_always_and_cleans_up() {
    if !nix::unistd::Uid::effective().is_root() {
        return;
    }
    let mut fixture = Fixture::new(BECOME_CHILD_WAITING_FOR_SIGNAL);
    // The become user writes the marker and opens the lifeline.
    fs::set_permissions(fixture.dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
    fs::set_permissions(fixture.path("lifeline"), fs::Permissions::from_mode(0o666)).unwrap();
    let args = ["--become-user".as_ref(), "nobody".as_ref()];
    assert_signal_stops_become_child(&mut fixture, &args);
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

#[test]
fn test_async_tasks_honor_check_mode() {
    let dir = tempdir().unwrap();
    let marker = dir.path().join("async-marker");
    let script_text = format!(
        r#"
#!/usr/bin/env rash
- command:
    argv: [touch, {marker}]
  async: 10
  poll: 1
  register: polled
- shell:
    cmd: touch {marker}
  loop: [a, b]
  async: 10
  poll: 0
- assert:
    that:
      - polled.changed
- debug:
    msg: async-check-ok
"#,
        marker = marker.display()
    );

    let (stdout, stderr) = run_test(&script_text, &["--check"]);

    assert!(stdout.contains("async-check-ok"), "stderr: {stderr}");
    assert!(!marker.exists(), "async job ran in check mode");
}
