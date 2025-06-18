/// ANCHOR: lookup
/// # pipe
///
/// Run a command and return the output.
///
/// ## Parameters
///
/// | Parameter | Required | Type   | Values | Description                                                                         |
/// | --------- | -------- | ------ | ------ | ----------------------------------------------------------------------------------- |
/// | command   | yes      | string |        | The command to run.                                                                 |
///
/// ## Notes
///
/// - Like all lookups this runs on the local machine and is unaffected by other keywords.
/// - Pipe lookup internally invokes commands with shell=True. This type of invocation is considered a security issue if appropriate care is not taken to sanitize any user provided or variable input.
/// - The directory of the play is used as the current working directory.
///
/// ANCHOR_END: lookup
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: raw result of running date command
///   debug:
///     msg: "{{ pipe('date') }}"
///
/// - name: Get current user
///   debug:
///     msg: "Current user: {{ pipe('whoami') }}"
///
/// - name: List files in directory
///   debug:
///     msg: "{{ pipe('ls -la /tmp') }}"
///
/// - name: Get system uptime
///   debug:
///     msg: "System uptime: {{ pipe('uptime') }}"
///
/// - name: Multiple commands with loop
///   debug:
///     msg: "{{ item }}"
///   loop:
///     - "{{ pipe('whoami') }}"
///     - "{{ pipe('pwd') }}"
///     - "{{ pipe('date') }}"
/// ```
/// ANCHOR_END: examples
use std::process::Command as StdCommand;
use std::result::Result as StdResult;

use log::trace;
use minijinja::{Error as MinijinjaError, ErrorKind as MinijinjaErrorKind, Value};

pub fn function(command: String) -> StdResult<Value, MinijinjaError> {
    trace!("pipe lookup - executing command: '{}'", command);

    // Execute the command using shell
    let output = StdCommand::new("/bin/sh")
        .arg("-c")
        .arg(&command)
        .output()
        .map_err(|e| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("Failed to execute command '{}': {}", command, e),
            )
        })?;

    trace!("pipe lookup - command output: {:?}", output);

    // Check if the command was successful
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!(
                "Command '{}' failed with exit code {:?}: {}",
                command,
                output.status.code(),
                stderr
            ),
        ));
    }

    // Return stdout as a string, trimming trailing newlines
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result = stdout.trim_end().to_string();

    trace!("pipe lookup - returning: '{}'", result);
    Ok(Value::from(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe_simple_command() {
        let result = function("echo 'hello world'".to_string());
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value.as_str().unwrap(), "hello world");
    }

    #[test]
    fn test_pipe_command_with_newlines() {
        let result = function("printf 'line1\\nline2\\n'".to_string());
        assert!(result.is_ok());
        let value = result.unwrap();
        // Should trim trailing newlines
        assert_eq!(value.as_str().unwrap(), "line1\nline2");
    }

    #[test]
    fn test_pipe_pwd() {
        let result = function("pwd".to_string());
        assert!(result.is_ok());
        let value = result.unwrap();
        // Should return current directory (starts with /)
        assert!(value.as_str().unwrap().starts_with('/'));
    }

    #[test]
    fn test_pipe_failed_command() {
        let result = function("exit 1".to_string());
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("failed with exit code"));
    }

    #[test]
    fn test_pipe_nonexistent_command() {
        let result = function("nonexistent_command_12345".to_string());
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("failed with exit code"));
    }

    #[test]
    fn test_pipe_empty_output() {
        // Command that produces no output
        let result = function("true".to_string());
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value.as_str().unwrap(), "");
    }
}
