/// ANCHOR: module
/// # assemble
///
/// Assemble configuration files from fragments.
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
/// - name: Assemble from fragments from a directory
///   assemble:
///     src: /etc/someapp/fragments
///     dest: /etc/someapp/someapp.conf
///
/// - name: Insert the provided delimiter between fragments
///   assemble:
///     src: /etc/someapp/fragments
///     dest: /etc/someapp/someapp.conf
///     delimiter: '### START FRAGMENT ###'
///
/// - name: Assemble a new "sshd_config" file into place, after passing validation
///   assemble:
///     src: /etc/ssh/conf.d/
///     dest: /etc/ssh/sshd_config
///     validate: /usr/sbin/sshd -t -f %s
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::parse_octal;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{File, OpenOptions, create_dir_all, metadata, read_dir, set_permissions};
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

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
    /// An already existing directory full of source files.
    pub src: String,
    /// A file to create using the concatenation of all of the source files.
    pub dest: String,
    /// A delimiter to separate the file contents.
    pub delimiter: Option<String>,
    /// The validation command to run before copying into place.
    /// The path to the file to validate is passed in by `%s` which must be present.
    pub validate: Option<String>,
    /// Assemble files only if the given regular expression matches the filename.
    /// If not set, all files are assembled.
    pub regexp: Option<String>,
    /// A boolean that controls if files that start with a `.` will be included or not.
    /// **[default: `false`]**
    #[serde(default)]
    pub ignore_hidden: bool,
    /// Permissions of the destination file.
    pub mode: Option<String>,
}

fn get_fragment_files(
    src: &Path,
    regexp: Option<&str>,
    ignore_hidden: bool,
) -> Result<Vec<String>> {
    let regex = regexp
        .map(|r| {
            Regex::new(r)
                .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid regexp: {e}")))
        })
        .transpose()?;

    let mut files = Vec::new();

    let entries = read_dir(src).map_err(|e| {
        Error::new(
            ErrorKind::NotFound,
            format!("Cannot read source directory '{}': {}", src.display(), e),
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| Error::new(ErrorKind::IOError, e))?;
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid filename in source directory: {}", path.display()),
                )
            })?
            .to_string();

        if !path.is_file() {
            continue;
        }

        if ignore_hidden && file_name.starts_with('.') {
            continue;
        }

        if let Some(ref re) = regex
            && !re.is_match(&file_name)
        {
            continue;
        }

        files.push(file_name);
    }

    files.sort();
    Ok(files)
}

fn read_fragment(src: &Path, filename: &str) -> Result<String> {
    let file_path = src.join(filename);
    let mut content = String::new();
    File::open(&file_path)
        .map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Cannot read '{}': {}", file_path.display(), e),
            )
        })?
        .read_to_string(&mut content)?;
    Ok(content)
}

fn run_validate_command(validate_cmd: &str, dest: &str) -> Result<()> {
    if !validate_cmd.contains("%s") {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "validate command must contain %s placeholder for the file path",
        ));
    }

    let cmd_str = validate_cmd.replace("%s", dest);

    let output = Command::new("/bin/sh")
        .args(["-c", &cmd_str])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Validation command failed: {}", stderr),
        ));
    }

    Ok(())
}

fn assemble_fragments(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let src_path = Path::new(&params.src);
    let dest_path = Path::new(&params.dest);

    if !src_path.is_dir() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Source '{}' is not a directory", params.src),
        ));
    }

    let fragment_files =
        get_fragment_files(src_path, params.regexp.as_deref(), params.ignore_hidden)?;

    if fragment_files.is_empty() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("No files found in source directory '{}'", params.src),
        ));
    }

    let mut assembled_content = String::new();
    let delimiter = params.delimiter.as_deref().unwrap_or("");

    for (i, filename) in fragment_files.iter().enumerate() {
        if i > 0 && !delimiter.is_empty() {
            assembled_content.push_str(delimiter);
            assembled_content.push('\n');
        }
        let content = read_fragment(src_path, filename)?;
        assembled_content.push_str(&content);
        if !content.ends_with('\n') {
            assembled_content.push('\n');
        }
    }

    let existing_content = if dest_path.exists() {
        let mut content = String::new();
        File::open(dest_path)
            .map_err(|e| Error::new(ErrorKind::IOError, e))?
            .read_to_string(&mut content)?;
        content
    } else {
        String::new()
    };

    if assembled_content == existing_content {
        return Ok(ModuleResult {
            changed: false,
            output: Some(params.dest),
            extra: None,
        });
    }

    diff(&existing_content, &assembled_content);

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(params.dest),
            extra: None,
        });
    }

    if let Some(parent) = dest_path.parent()
        && !parent.exists()
    {
        create_dir_all(parent)?;
    }

    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(dest_path)?;

        file.write_all(assembled_content.as_bytes())?;
    }

    if let Some(mode) = &params.mode {
        let octal_mode = parse_octal(mode)?;
        let mut permissions = metadata(dest_path)?.permissions();
        permissions.set_mode(octal_mode);
        set_permissions(dest_path, permissions)?;
    }

    if let Some(validate) = &params.validate {
        run_validate_command(validate, &params.dest)?;
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(params.dest),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Assemble;

impl Module for Assemble {
    fn get_name(&self) -> &str {
        "assemble"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            assemble_fragments(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::{create_dir, write};

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /etc/someapp/fragments
            dest: /etc/someapp/someapp.conf
            delimiter: '### START FRAGMENT ###'
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/etc/someapp/fragments".to_owned(),
                dest: "/etc/someapp/someapp.conf".to_owned(),
                delimiter: Some("### START FRAGMENT ###".to_owned()),
                validate: None,
                regexp: None,
                ignore_hidden: false,
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /etc/fragments
            dest: /etc/output.conf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/etc/fragments".to_owned(),
                dest: "/etc/output.conf".to_owned(),
                delimiter: None,
                validate: None,
                regexp: None,
                ignore_hidden: false,
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_regexp() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /etc/fragments
            dest: /etc/output.conf
            regexp: '.*\.conf$'
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.regexp, Some(".*\\.conf$".to_owned()));
    }

    #[test]
    fn test_get_fragment_files() {
        let dir = tempdir().unwrap();
        let src = dir.path();

        write(src.join("00-header.conf"), "header content").unwrap();
        write(src.join("01-body.conf"), "body content").unwrap();
        write(src.join("02-footer.conf"), "footer content").unwrap();
        create_dir(src.join("subdir")).unwrap();
        write(src.join(".hidden"), "hidden content").unwrap();

        let files = get_fragment_files(src, None, false).unwrap();
        assert_eq!(
            files,
            vec![
                ".hidden",
                "00-header.conf",
                "01-body.conf",
                "02-footer.conf"
            ]
        );

        let files = get_fragment_files(src, None, true).unwrap();
        assert_eq!(
            files,
            vec!["00-header.conf", "01-body.conf", "02-footer.conf"]
        );

        let files = get_fragment_files(src, Some(r".*\.conf$"), false).unwrap();
        assert_eq!(
            files,
            vec!["00-header.conf", "01-body.conf", "02-footer.conf"]
        );
    }

    #[test]
    fn test_assemble_fragments_basic() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content\n").unwrap();
        write(src.join("20-second.txt"), "second content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false).unwrap();
        assert!(result.changed);

        let output = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(output, "first content\nsecond content\n");
    }

    #[test]
    fn test_assemble_fragments_with_delimiter() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content").unwrap();
        write(src.join("20-second.txt"), "second content").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: Some("# --- fragment ---".to_owned()),
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false).unwrap();
        assert!(result.changed);

        let output = std::fs::read_to_string(&dest).unwrap();
        assert!(output.contains("# --- fragment ---"));
        assert!(output.contains("first content"));
        assert!(output.contains("second content"));
    }

    #[test]
    fn test_assemble_fragments_no_change() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content\n").unwrap();

        write(&dest, "first content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_assemble_fragments_check_mode() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, true).unwrap();
        assert!(result.changed);
        assert!(!dest.exists());
    }

    #[test]
    fn test_assemble_fragments_with_mode() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: Some("0600".to_owned()),
        };

        let result = assemble_fragments(params, false).unwrap();
        assert!(result.changed);

        let permissions = metadata(&dest).unwrap().permissions();
        assert_eq!(permissions.mode() & 0o7777, 0o600);
    }

    #[test]
    fn test_assemble_fragments_create_parent_dirs() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("subdir").join("deep").join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false).unwrap();
        assert!(result.changed);
        assert!(dest.exists());
    }

    #[test]
    fn test_assemble_fragments_source_not_directory() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("notadir");
        let dest = dir.path().join("output.conf");

        write(&src, "some content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a directory"));
    }

    #[test]
    fn test_assemble_fragments_empty_directory() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: None,
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No files found"));
    }

    #[test]
    fn test_assemble_fragments_validate_success() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: Some("test -f %s".to_owned()),
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false).unwrap();
        assert!(result.changed);
    }

    #[test]
    fn test_assemble_fragments_validate_missing_placeholder() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("fragments");
        let dest = dir.path().join("output.conf");

        create_dir(&src).unwrap();
        write(src.join("10-first.txt"), "first content\n").unwrap();

        let params = Params {
            src: src.to_str().unwrap().to_owned(),
            dest: dest.to_str().unwrap().to_owned(),
            delimiter: None,
            validate: Some("echo hello".to_owned()),
            regexp: None,
            ignore_hidden: false,
            mode: None,
        };

        let result = assemble_fragments(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must contain %s"));
    }
}
