/// ANCHOR: module
/// # expect
///
/// Executes a command and responds to prompts.
///
/// This module automates interactive commands by matching prompts using
/// regular expressions and sending responses.
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
/// - name: Automate password change
///   expect:
///     command: passwd testuser
///     responses:
///       "(?i)password:": "MySekretPa$$word"
///
/// - name: Multiple prompts with responses
///   expect:
///     command: /path/to/interactive/script.sh
///     responses:
///       "Enter name:":
///         - "Alice"
///       "Enter email:":
///         - "alice@example.com"
///     timeout: 60
///
/// - name: Run in specific directory
///   expect:
///     command: ./setup.sh
///     chdir: /opt/app
///     responses:
///       "Continue\\? \\[y/N\\]": "y"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use expectrl::{Any, Regex, Session, WaitStatus};
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

const DEFAULT_TIMEOUT: u64 = 30;

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The command to execute.
    pub command: String,
    /// Mapping of prompt regular expressions and corresponding answer(s).
    /// The key is a regex pattern to match prompts.
    /// The value is either a single response string or a list of responses
    /// for successive matches.
    pub responses: HashMap<String, Response>,
    /// Change into this directory before running the command.
    pub chdir: Option<String>,
    /// A filename, when it already exists, this step will not be run.
    pub creates: Option<String>,
    /// A filename, when it does not exist, this step will not be run.
    pub removes: Option<String>,
    /// Amount of time in seconds to wait for the expected strings.
    /// Use 0 to disable timeout.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum Response {
    Single(String),
    Multiple(Vec<String>),
}

impl Response {
    pub fn get_response(&self, index: usize) -> Option<&str> {
        match self {
            Response::Single(s) => Some(s),
            Response::Multiple(v) => v.get(index).map(|s| s.as_str()),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Response::Single(_) => 1,
            Response::Multiple(v) => v.len(),
        }
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

struct ResponseTracker {
    patterns: Vec<String>,
    responses: HashMap<String, (Response, usize)>,
}

impl ResponseTracker {
    fn new(responses: HashMap<String, Response>) -> Self {
        let patterns: Vec<String> = responses.keys().cloned().collect();
        let tracked = responses.into_iter().map(|(k, v)| (k, (v, 0))).collect();
        Self {
            patterns,
            responses: tracked,
        }
    }

    fn get_response(&mut self, pattern: &str) -> Option<&str> {
        let (response, index) = self.responses.get_mut(pattern)?;
        let result = response.get_response(*index);
        if response.len() > 1 && *index < response.len() - 1 {
            *index += 1;
        }
        result
    }

    fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

fn check_creates(creates: &Option<String>) -> Result<bool> {
    if let Some(path) = creates
        && Path::new(path).exists()
    {
        debug!("File {} exists, skipping expect command", path);
        return Ok(true);
    }
    Ok(false)
}

fn check_removes(removes: &Option<String>) -> Result<bool> {
    if let Some(path) = removes
        && !Path::new(path).exists()
    {
        debug!("File {} does not exist, skipping expect command", path);
        return Ok(true);
    }
    Ok(false)
}

fn build_command(command: &str, chdir: Option<&str>) -> Result<Command> {
    let argv: Vec<&str> = command.split_whitespace().collect();
    if argv.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "command cannot be empty",
        ));
    }

    let mut cmd = Command::new(argv[0]);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    if let Some(dir) = chdir {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}

fn run_expect(params: Params) -> Result<(ModuleResult, Option<Value>)> {
    if check_creates(&params.creates)? {
        return Ok((ModuleResult::new(false, None, None), None));
    }

    if check_removes(&params.removes)? {
        return Ok((ModuleResult::new(false, None, None), None));
    }

    let cmd = build_command(&params.command, params.chdir.as_deref())?;
    let mut session = Session::spawn(cmd).map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if params.timeout > 0 {
        session.set_expect_timeout(Some(Duration::from_secs(params.timeout)));
    }

    let mut tracker = ResponseTracker::new(params.responses);
    let mut full_output = String::new();

    let patterns = tracker.patterns();
    if patterns.is_empty() {
        session
            .read_to_string(&mut full_output)
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    } else {
        let owned_patterns: Vec<String> = patterns.to_vec();
        loop {
            let needles: Vec<Box<dyn expectrl::Needle>> = owned_patterns
                .iter()
                .map(|p| Box::new(Regex(p.clone())) as Box<dyn expectrl::Needle>)
                .collect();

            let any = Any::boxed(needles);

            match session.expect(any) {
                Ok(captures) => {
                    let before = captures.before();
                    full_output.push_str(&String::from_utf8_lossy(before));

                    if let Some(matched) = captures.get(0) {
                        let matched_str = String::from_utf8_lossy(matched);
                        full_output.push_str(&matched_str);

                        for pattern in &owned_patterns {
                            if let Ok(re) = regex::Regex::new(pattern)
                                && re.is_match(&matched_str)
                            {
                                if let Some(response) = tracker.get_response(pattern) {
                                    session
                                        .send_line(response)
                                        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
                                    full_output.push_str(response);
                                    full_output.push('\n');
                                }
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let remaining = session
                        .fill_buf()
                        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
                    full_output.push_str(&String::from_utf8_lossy(remaining));
                    let len = remaining.len();
                    session.consume(len);

                    if let expectrl::Error::Eof = e {
                        break;
                    }
                    if matches!(e, expectrl::Error::ExpectTimeout) {
                        return Err(Error::new(
                            ErrorKind::SubprocessFail,
                            format!("Timeout waiting for expected pattern: {}", e),
                        ));
                    }
                    return Err(Error::new(ErrorKind::SubprocessFail, e));
                }
            }
        }
    }

    let wait_status = session
        .get_process_mut()
        .wait()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    let rc = match wait_status {
        WaitStatus::Exited(_, code) => Some(code),
        _ => None,
    };

    let extra = Some(value::to_value(json!({
        "rc": rc,
    }))?);

    let output = if full_output.is_empty() {
        None
    } else {
        Some(full_output)
    };

    Ok((ModuleResult::new(true, extra, output), None))
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
    fn test_parse_params_single_response() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "passwd user"
            responses:
              "(?i)password:": "secret"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, "passwd user");
        assert_eq!(params.timeout, DEFAULT_TIMEOUT);
        assert!(params.responses.contains_key("(?i)password:"));
        assert_eq!(
            params.responses.get("(?i)password:"),
            Some(&Response::Single("secret".to_owned()))
        );
    }

    #[test]
    fn test_parse_params_multiple_responses() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "./script.sh"
            responses:
              "Enter name:":
                - "Alice"
                - "Bob"
            timeout: 60
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, "./script.sh");
        assert_eq!(params.timeout, 60);
        assert_eq!(
            params.responses.get("Enter name:"),
            Some(&Response::Multiple(vec![
                "Alice".to_owned(),
                "Bob".to_owned()
            ]))
        );
    }

    #[test]
    fn test_parse_params_with_chdir() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "./setup.sh"
            chdir: "/opt/app"
            responses:
              "Continue?": "y"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chdir, Some("/opt/app".to_owned()));
    }

    #[test]
    fn test_parse_params_with_creates() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "./setup.sh"
            creates: "/tmp/setup_done"
            responses:
              "Continue?": "y"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.creates, Some("/tmp/setup_done".to_owned()));
    }

    #[test]
    fn test_parse_params_with_removes() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "./cleanup.sh"
            removes: "/tmp/old_file"
            responses:
              "Confirm?": "y"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.removes, Some("/tmp/old_file".to_owned()));
    }

    #[test]
    fn test_parse_params_missing_required() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: "ls"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_response_tracker_single_response() {
        let mut responses = HashMap::new();
        responses.insert("prompt".to_owned(), Response::Single("answer".to_owned()));
        let mut tracker = ResponseTracker::new(responses);

        assert_eq!(tracker.get_response("prompt"), Some("answer"));
        assert_eq!(tracker.get_response("prompt"), Some("answer"));
    }

    #[test]
    fn test_response_tracker_multiple_responses() {
        let mut responses = HashMap::new();
        responses.insert(
            "prompt".to_owned(),
            Response::Multiple(vec!["ans1".to_owned(), "ans2".to_owned()]),
        );
        let mut tracker = ResponseTracker::new(responses);

        assert_eq!(tracker.get_response("prompt"), Some("ans1"));
        assert_eq!(tracker.get_response("prompt"), Some("ans2"));
        assert_eq!(tracker.get_response("prompt"), Some("ans2"));
    }

    #[test]
    fn test_build_command_basic() {
        let cmd = build_command("echo hello", None).unwrap();
        assert_eq!(cmd.get_program(), "echo");
    }

    #[test]
    fn test_build_command_empty() {
        let result = build_command("", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_creates_exists() {
        use std::fs::File;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("exists.txt");
        File::create(&file_path).unwrap();

        let result = check_creates(&Some(file_path.to_str().unwrap().to_owned())).unwrap();
        assert!(result);
    }

    #[test]
    fn test_check_creates_not_exists() {
        let result = check_creates(&Some("/nonexistent/path".to_owned())).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_check_removes_exists() {
        use std::fs::File;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("exists.txt");
        File::create(&file_path).unwrap();

        let result = check_removes(&Some(file_path.to_str().unwrap().to_owned())).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_check_removes_not_exists() {
        let result = check_removes(&Some("/nonexistent/path".to_owned())).unwrap();
        assert!(result);
    }
}
