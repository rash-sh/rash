/// ANCHOR: module
/// # expect
///
/// Execute interactive commands and automate responses.
///
/// This module automates interactive CLI programs by spawning a pseudo-terminal
/// and responding to prompts based on pattern matching.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Run interactive setup
///   expect:
///     command: /usr/local/bin/setup-wizard
///     responses:
///       "Enter password:": "{{ vault_password }}"
///       "Confirm (y/n):": "y"
///     timeout: 30
///
/// - name: Automate SSH key creation
///   expect:
///     command: ssh-keygen -t rsa -f /tmp/id_rsa
///     responses:
///       "Enter passphrase": ""
///       "Enter same passphrase": ""
///     timeout: 10
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd};
use std::os::unix::io::OwnedFd;
use std::process::exit;
use std::time::{Duration, Instant};

use nix::pty::{OpenptyResult, openpty};
use nix::sys::select::{FdSet, select};
use nix::unistd::{ForkResult, close, dup2, execvp, fork, setsid};

const DEFAULT_TIMEOUT: u64 = 30;
const BUFFER_SIZE: usize = 4096;

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT
}

fn default_echo() -> bool {
    false
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The command to run with interactive prompts.
    pub command: String,
    /// A dictionary mapping prompt patterns (strings or regex) to their responses.
    /// When a pattern is matched in the output, the corresponding response is sent.
    pub responses: HashMap<String, String>,
    /// Maximum time in seconds to wait for each expected pattern.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Whether to echo the command output to stdout.
    #[serde(default = "default_echo")]
    pub echo: bool,
}

struct ExpectSession {
    master_fd: OwnedFd,
    output_buffer: String,
}

impl ExpectSession {
    fn spawn(command: &str) -> Result<Self> {
        let OpenptyResult { master, slave } =
            openpty(None, None).map_err(|e| Error::new(ErrorKind::IOError, e))?;

        match unsafe { fork() } {
            Ok(ForkResult::Parent { child: _ }) => {
                close(slave.as_raw_fd()).map_err(|e| Error::new(ErrorKind::IOError, e))?;

                Ok(ExpectSession {
                    master_fd: master,
                    output_buffer: String::new(),
                })
            }
            Ok(ForkResult::Child) => {
                setsid().expect("setsid failed");

                let mut stdin_fd = unsafe { OwnedFd::from_raw_fd(0) };
                let mut stdout_fd = unsafe { OwnedFd::from_raw_fd(1) };
                let mut stderr_fd = unsafe { OwnedFd::from_raw_fd(2) };

                dup2(&slave, &mut stdin_fd).expect("dup2 stdin failed");
                dup2(&slave, &mut stdout_fd).expect("dup2 stdout failed");
                dup2(&slave, &mut stderr_fd).expect("dup2 stderr failed");

                std::mem::forget(stdin_fd);
                std::mem::forget(stdout_fd);
                std::mem::forget(stderr_fd);

                let parts: Vec<&str> = command.split_whitespace().collect();
                if parts.is_empty() {
                    exit(1);
                }

                let program = std::ffi::CString::new(parts[0]).expect("invalid program name");
                let args: Vec<std::ffi::CString> = parts
                    .iter()
                    .map(|s| std::ffi::CString::new(*s).expect("invalid arg"))
                    .collect();

                let err = execvp(&program, &args);
                eprintln!("Failed to execute {}: {:?}", parts[0], err);
                exit(127);
            }
            Err(e) => {
                let _ = close(master.as_raw_fd());
                let _ = close(slave.as_raw_fd());
                Err(Error::new(ErrorKind::SubprocessFail, e))
            }
        }
    }

    fn read_available(&mut self, timeout_ms: u64) -> Result<bool> {
        let mut fd_set = FdSet::new();
        fd_set.insert(self.master_fd.as_fd());

        let tv_sec = (timeout_ms / 1000) as nix::libc::time_t;
        let tv_usec = ((timeout_ms % 1000) * 1000) as nix::libc::suseconds_t;
        let mut timeout_val = nix::sys::time::TimeVal::new(tv_sec, tv_usec);

        let result = select(
            self.master_fd.as_raw_fd() + 1,
            Some(&mut fd_set),
            None,
            None,
            Some(&mut timeout_val),
        )
        .map_err(|e| Error::new(ErrorKind::IOError, e))?;

        if result > 0 && fd_set.contains(self.master_fd.as_fd()) {
            let mut buf = [0u8; BUFFER_SIZE];
            let mut master_file = unsafe { std::fs::File::from_raw_fd(self.master_fd.as_raw_fd()) };
            let n = master_file
                .read(&mut buf)
                .map_err(|e| Error::new(ErrorKind::IOError, e))?;

            std::mem::forget(master_file);

            if n == 0 {
                return Ok(false);
            }

            let output = String::from_utf8_lossy(&buf[..n]);
            self.output_buffer.push_str(&output);
            return Ok(true);
        }

        Ok(false)
    }

    fn expect(&mut self, pattern: &str, timeout_secs: u64, echo: bool) -> Result<bool> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        loop {
            if self.output_buffer.contains(pattern) {
                return Ok(true);
            }

            if start.elapsed() >= timeout {
                return Ok(false);
            }

            let remaining_ms = (timeout - start.elapsed()).as_millis() as u64;
            let read_timeout = remaining_ms.min(100);

            match self.read_available(read_timeout)? {
                true => {
                    if echo {
                        let last_newline = self.output_buffer.rfind('\n');
                        if let Some(pos) = last_newline {
                            print!(
                                "{}",
                                &self.output_buffer[self.output_buffer.len()
                                    - (self.output_buffer.len() - pos - 1)..]
                            );
                            std::io::stdout().flush().ok();
                        }
                    }
                }
                false => {
                    if !self.output_buffer.contains(pattern) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        }
    }

    fn send(&mut self, response: &str) -> Result<()> {
        let mut master_file = unsafe { std::fs::File::from_raw_fd(self.master_fd.as_raw_fd()) };
        master_file
            .write_all(response.as_bytes())
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        master_file
            .write_all(b"\n")
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        master_file
            .flush()
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        std::mem::forget(master_file);
        Ok(())
    }

    fn drain_remaining_output(&mut self, timeout_ms: u64, echo: bool) -> Result<String> {
        let start = Instant::now();
        let drain_timeout = Duration::from_millis(timeout_ms);

        loop {
            if start.elapsed() >= drain_timeout {
                break;
            }

            let remaining_ms = (drain_timeout - start.elapsed()).as_millis() as u64;
            let read_timeout = remaining_ms.min(50);

            if !self.read_available(read_timeout)? {
                break;
            }
        }

        if echo {
            print!("{}", self.output_buffer);
            std::io::stdout().flush().ok();
        }

        Ok(self.output_buffer.clone())
    }

    fn close(self) -> Result<()> {
        close(self.master_fd.as_raw_fd()).map_err(|e| Error::new(ErrorKind::IOError, e))?;
        Ok(())
    }
}

fn run_expect(params: Params) -> Result<(ModuleResult, Option<Value>)> {
    let mut session = ExpectSession::spawn(&params.command)?;

    let mut matched_patterns = Vec::new();

    for (pattern, response) in &params.responses {
        if !session.expect(pattern, params.timeout, params.echo)? {
            let full_output = session.output_buffer.clone();
            let _ = session.close();

            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Timeout waiting for pattern: '{}'\nOutput so far:\n{}",
                    pattern, full_output
                ),
            ));
        }

        matched_patterns.push(pattern.clone());
        session.send(response)?;
    }

    let full_output = session.drain_remaining_output(500, params.echo)?;
    session.close()?;

    let extra = Some(value::to_value(json!({
        "output": full_output,
        "matched_patterns": matched_patterns,
    }))?);

    Ok((ModuleResult::new(true, extra, None), None))
}

#[derive(Debug)]
pub struct Expect;

impl Module for Expect {
    fn get_name(&self) -> &str {
        "expect"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        run_expect(params)
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "echo test"
            responses:
              "prompt:": "response"
            timeout: 10
            echo: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, "echo test");
        assert_eq!(params.timeout, 10);
        assert!(params.echo);
        assert_eq!(
            params.responses.get("prompt:"),
            Some(&"response".to_string())
        );
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "echo test"
            responses:
              "prompt:": "response"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.timeout, DEFAULT_TIMEOUT);
        assert!(!params.echo);
    }

    #[test]
    fn test_parse_params_missing_command() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            responses:
              "prompt:": "response"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_responses() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "echo test"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
