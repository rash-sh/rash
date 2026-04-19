/// ANCHOR: module
/// # replace
///
/// Replace all instances of a particular string in a file using a back-referenced regular expression.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - replace:
///     path: /etc/hosts
///     regexp: '(\s+)old\.host\.name(\s+.*)?$'
///     replace: '\1new.host.name\2'
///
/// - replace:
///     path: /etc/apache2/sites-available/default.conf
///     after: 'NameVirtualHost [*]'
///     regexp: '^(.+)$'
///     replace: '# \1'
///
/// - replace:
///     path: /etc/ssh/sshd_config
///     regexp: '^(ListenAddress[ ]+)[^\n]+$'
///     replace: '\g<1>0.0.0.0'
///     backup: true
///
/// - replace:
///     path: /etc/apache/ports
///     regexp: '^(NameVirtualHost|Listen)\s+80\s*$'
///     replace: '\1 127.0.0.1:8080'
///     validate: '/usr/sbin/apache2ctl -f %s -t'
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::ffi::OsStr;
use std::fs::{File, OpenOptions, read_to_string};
use std::io::prelude::*;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use minijinja::Value;
use regex::Regex;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The file to modify.
    pub path: String,
    /// The regular expression to look for in the contents of the file.
    /// Uses Python regular expressions; see http://docs.python.org/3/library/re.html.
    /// Uses MULTILINE mode, which means ^ and $ match the beginning and end
    /// of each line of the file, as well as the beginning and end of the file.
    pub regexp: String,
    /// The string to replace regexp matches.
    /// May contain backreferences that will get expanded with the regexp capture groups
    /// if the regexp matches. If not set, matches are removed entirely.
    /// **[default: `""`]**
    pub replace: Option<String>,
    /// Create a backup file including the timestamp information so you can get
    /// the original file back if you somehow clobbered it incorrectly.
    /// **[default: `false`]**
    pub backup: Option<bool>,
    /// The validation command to run before copying the updated file into the final destination.
    /// A temporary file path is used to validate, passed in through %s which must be present.
    pub validate: Option<String>,
    /// If specified, only content after this match will be replaced/removed.
    /// Can be used in combination with before.
    /// Uses Python regular expressions; see http://docs.python.org/3/library/re.html.
    /// Uses DOTALL, which means the . special character can match newlines.
    pub after: Option<String>,
    /// If specified, only content before this match will be replaced/removed.
    /// Can be used in combination with after.
    /// Uses Python regular expressions; see http://docs.python.org/3/library/re.html.
    /// Uses DOTALL, which means the . special character can match newlines.
    pub before: Option<String>,
    /// The character encoding for reading and writing the file.
    /// **[default: `"utf-8"`]**
    pub encoding: Option<String>,
}

fn create_backup(path: &Path) -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_path = format!("{}.{}", path.display(), timestamp);
    std::fs::copy(path, &backup_path)?;
    trace!("created backup: {}", backup_path);
    Ok(backup_path)
}

fn run_validate(validate_cmd: &str, temp_path: &Path) -> Result<()> {
    let cmd_with_path = validate_cmd.replace("%s", temp_path.to_str().unwrap_or(""));
    let parts = shlex::split(&cmd_with_path)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Failed to parse validate command"))?;

    if parts.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "validate command must contain %s placeholder",
        ));
    }

    let program = &parts[0];
    let args = &parts[1..];
    let output = Command::new(program).args(args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute validate command: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Validation failed: {}", stderr.trim()),
        ));
    }

    Ok(())
}

fn apply_context_boundaries(
    content: &str,
    after: Option<&str>,
    before: Option<&str>,
) -> Result<(String, usize, usize)> {
    let mut start_idx = 0;
    let mut end_idx = content.len();

    if let Some(after_pattern) = after {
        let dotall_regex = Regex::new(&format!("(?s){}", after_pattern)).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid after regexp: {}", e),
            )
        })?;

        if let Some(m) = dotall_regex.find(content) {
            start_idx = m.end();
        }
    }

    if let Some(before_pattern) = before {
        let dotall_regex = Regex::new(&format!("(?s){}", before_pattern)).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid before regexp: {}", e),
            )
        })?;

        if let Some(m) = dotall_regex.find(content[start_idx..].as_ref()) {
            end_idx = start_idx + m.start();
        }
    }

    Ok((content[start_idx..end_idx].to_string(), start_idx, end_idx))
}

pub fn replace(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let path = Path::new(&params.path);

    if !path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("File {} does not exist", params.path),
        ));
    }

    let original_content = read_to_string(path)?;

    let main_regex = Regex::new(&params.regexp)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid regexp: {}", e)))?;

    let (context_content, start_idx, end_idx) = apply_context_boundaries(
        &original_content,
        params.after.as_deref(),
        params.before.as_deref(),
    )?;

    let replacement = params.replace.as_deref().unwrap_or("");
    let new_context_content = main_regex
        .replace_all(&context_content, replacement)
        .to_string();

    if context_content == new_context_content {
        return Ok(ModuleResult {
            changed: false,
            output: Some(params.path.clone()),
            extra: None,
        });
    }

    let new_content = if start_idx == 0 && end_idx == original_content.len() {
        new_context_content
    } else {
        let mut result = String::new();
        if start_idx > 0 {
            result.push_str(&original_content[..start_idx]);
        }
        result.push_str(&new_context_content);
        if end_idx < original_content.len() {
            result.push_str(&original_content[end_idx..]);
        }
        result
    };

    diff(&original_content, &new_content);

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(params.path.clone()),
            extra: None,
        });
    }

    if params.backup.unwrap_or(false) {
        create_backup(path)?;
    }

    if let Some(validate_cmd) = &params.validate {
        let temp_dir = tempfile::tempdir().map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to create temp dir: {}", e),
            )
        })?;
        let temp_file_path = temp_dir
            .path()
            .join(path.file_name().unwrap_or_else(|| OsStr::new("tempfile")));

        let mut temp_file = File::create(&temp_file_path).map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to create temp file: {}", e),
            )
        })?;
        temp_file.write_all(new_content.as_bytes())?;

        run_validate(validate_cmd, &temp_file_path)?;
    }

    let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
    file.write_all(new_content.as_bytes())?;

    Ok(ModuleResult {
        changed: true,
        output: Some(params.path.clone()),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Replace;

impl Module for Replace {
    fn get_name(&self) -> &str {
        "replace"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((replace(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "/tmp/test.txt"
            regexp: "^test"
            replace: "new"
            backup: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/tmp/test.txt".to_owned(),
                regexp: "^test".to_owned(),
                replace: Some("new".to_owned()),
                backup: Some(true),
                validate: None,
                after: None,
                before: None,
                encoding: None,
            }
        );
    }

    #[test]
    fn test_replace_simple() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "hello world\nhello universe\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "hello".to_string(),
            replace: Some("hi".to_string()),
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hi world\nhi universe\n");
    }

    #[test]
    fn test_replace_with_backreference() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "foo=bar\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "foo=(.*)".to_string(),
            replace: Some("foo=${1}_new".to_string()),
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "foo=bar_new\n");
    }

    #[test]
    fn test_replace_no_match() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "hello world\n").unwrap();
        let original = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "xyz".to_string(),
            replace: Some("abc".to_string()),
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false).unwrap();
        assert!(!result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_replace_remove_matches() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "abc123def\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "\\d+".to_string(),
            replace: None,
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "abcdef\n");
    }

    #[test]
    fn test_replace_with_after() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "header\ncontent1\ncontent2\nfooter\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "content".to_string(),
            replace: Some("new".to_string()),
            backup: None,
            validate: None,
            after: Some("header".to_string()),
            before: Some("footer".to_string()),
            encoding: None,
        };

        let result = replace(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "header\nnew1\nnew2\nfooter\n");
    }

    #[test]
    fn test_replace_with_backup() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "original content\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "original".to_string(),
            replace: Some("new".to_string()),
            backup: Some(true),
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content\n");

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                name.to_str().map(|s| s != "test.txt").unwrap_or(false)
            })
            .collect();
        assert_eq!(backup_files.len(), 1);
        assert!(
            fs::read_to_string(backup_files[0].path())
                .unwrap()
                .contains("original")
        );
    }

    #[test]
    fn test_replace_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "hello world\n").unwrap();
        let original = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "hello".to_string(),
            replace: Some("hi".to_string()),
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, true).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_replace_file_not_found() {
        let params = Params {
            path: "/nonexistent/file.txt".to_string(),
            regexp: "test".to_string(),
            replace: Some("new".to_string()),
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_replace_invalid_regexp() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "test content\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "[invalid".to_string(),
            replace: Some("new".to_string()),
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regexp"));
    }

    #[test]
    fn test_replace_multiline() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            regexp: "(?m)^line".to_string(),
            replace: Some("new".to_string()),
            backup: None,
            validate: None,
            after: None,
            before: None,
            encoding: None,
        };

        let result = replace(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new1\nnew2\nnew3\n");
    }
}
