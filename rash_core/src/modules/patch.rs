/// ANCHOR: module
/// # patch
///
/// Apply patch files to source files using the system `patch` command.
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
/// - name: Apply security patch to application
///   patch:
///     src: /tmp/security.patch
///     dest: /opt/app/src/main.rs
///     backup: true
///
/// - name: Apply patch with stripped leading paths
///   patch:
///     src: /tmp/fix.patch
///     dest: /opt/app/src/config.rs
///     strip: 1
///
/// - name: Test patch without applying
///   patch:
///     src: /tmp/test.patch
///     dest: /opt/app/src/main.rs
///     dry_run: true
///
/// - name: Reverse a previously applied patch
///   patch:
///     src: /tmp/rollback.patch
///     dest: /opt/app/src/main.rs
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the patch file to apply.
    pub src: String,
    /// Destination file to patch.
    pub dest: String,
    /// Base directory for applying the patch.
    pub basedir: Option<String>,
    /// Whether the patch should be applied or reversed.
    /// `present` applies the patch, `absent` reverses it.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Create a backup of the original file before patching.
    /// **[default: `false`]**
    pub backup: Option<bool>,
    /// Test the patch without actually applying it.
    /// **[default: `false`]**
    pub dry_run: Option<bool>,
    /// Number of leading path components to strip from file paths in the patch.
    /// **[default: `0`]**
    pub strip: Option<u32>,
}

fn build_patch_command(params: &Params, dry_run_force: bool) -> Result<Command> {
    let mut cmd = Command::new("patch");

    if dry_run_force || params.dry_run.unwrap_or(false) {
        cmd.arg("--dry-run");
    }

    if params.backup.unwrap_or(false) && !dry_run_force {
        cmd.arg("--backup");
    } else if !dry_run_force {
        cmd.arg("--no-backup-if-mismatch");
    }

    if let Some(strip) = params.strip {
        cmd.arg(format!("-p{}", strip));
    }

    let state = params.state.as_ref().unwrap_or(&State::Present);
    if *state == State::Absent {
        cmd.arg("--reverse");
    }

    cmd.arg("--input").arg(&params.src);
    cmd.arg(&params.dest);

    if let Some(ref basedir) = params.basedir {
        let basedir_path = Path::new(basedir);
        if !basedir_path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Basedir {} does not exist", basedir),
            ));
        }
        cmd.current_dir(basedir_path);
    }

    Ok(cmd)
}

fn run_patch(params: &Params, check_mode: bool) -> Result<(bool, String)> {
    let src_path = Path::new(&params.src);
    if !src_path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Patch file {} does not exist", params.src),
        ));
    }

    let dest_path = Path::new(&params.dest);
    if !dest_path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Destination file {} does not exist", params.dest),
        ));
    }

    let is_dry_run = check_mode || params.dry_run.unwrap_or(false);
    let mut cmd = build_patch_command(params, is_dry_run)?;
    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute patch command: {}", e),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let state = params.state.as_ref().unwrap_or(&State::Present);
        let action = match state {
            State::Present => "apply",
            State::Absent => "reverse",
        };
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Patch command failed (cannot {} patch): {}{}",
                action,
                stdout.trim(),
                stderr.trim()
            ),
        ));
    }

    let changed = stdout.contains("patching")
        || stdout.contains("checking file")
        || stderr.contains("patching")
        || stderr.contains("checking file");

    Ok((changed, stdout))
}

pub fn patch(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let (changed, output) = run_patch(&params, check_mode)?;

    Ok(ModuleResult {
        changed,
        output: Some(output.trim().to_string()),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Patch;

impl Module for Patch {
    fn get_name(&self) -> &str {
        "patch"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((patch(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: "/tmp/fix.patch"
            dest: "/opt/app/main.rs"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/tmp/fix.patch".to_owned(),
                dest: "/opt/app/main.rs".to_owned(),
                basedir: None,
                state: None,
                backup: None,
                dry_run: None,
                strip: None,
            }
        );
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: "/tmp/fix.patch"
            dest: "/opt/app/main.rs"
            basedir: "/opt/app"
            state: present
            backup: true
            dry_run: false
            strip: 1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.src, "/tmp/fix.patch");
        assert_eq!(params.dest, "/opt/app/main.rs");
        assert_eq!(params.basedir, Some("/opt/app".to_owned()));
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.backup, Some(true));
        assert_eq!(params.dry_run, Some(false));
        assert_eq!(params.strip, Some(1));
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: "/tmp/fix.patch"
            dest: "/opt/app/main.rs"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_patch_src_not_found() {
        let params = Params {
            src: "/nonexistent/fix.patch".to_owned(),
            dest: "/opt/app/main.rs".to_owned(),
            basedir: None,
            state: None,
            backup: None,
            dry_run: None,
            strip: None,
        };
        let result = patch(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_patch_dest_not_found() {
        let dir = tempdir().unwrap();
        let patch_path = dir.path().join("fix.patch");
        fs::write(&patch_path, "--- a/test.txt\n+++ b/test.txt\n").unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: "/nonexistent/file.txt".to_owned(),
            basedir: None,
            state: None,
            backup: None,
            dry_run: None,
            strip: None,
        };
        let result = patch(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_patch_apply_simple() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello world\n").unwrap();
        fs::write(
            &patch_path,
            "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
        )
        .unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: None,
            state: None,
            backup: None,
            dry_run: None,
            strip: None,
        };

        let result = patch(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello universe\n");
    }

    #[test]
    fn test_patch_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello world\n").unwrap();
        fs::write(
            &patch_path,
            "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
        )
        .unwrap();

        let original = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: None,
            state: None,
            backup: None,
            dry_run: None,
            strip: None,
        };

        let result = patch(params, true).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_patch_dry_run() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello world\n").unwrap();
        fs::write(
            &patch_path,
            "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
        )
        .unwrap();

        let original = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: None,
            state: None,
            backup: None,
            dry_run: Some(true),
            strip: None,
        };

        let result = patch(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_patch_with_backup() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello world\n").unwrap();
        fs::write(
            &patch_path,
            "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
        )
        .unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: None,
            state: None,
            backup: Some(true),
            dry_run: None,
            strip: None,
        };

        let result = patch(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello universe\n");

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                name.to_str()
                    .map(|s| s != "test.txt" && s != "test.patch")
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(backup_files.len(), 1);
        assert!(
            fs::read_to_string(backup_files[0].path())
                .unwrap()
                .contains("hello world")
        );
    }

    #[test]
    fn test_patch_reverse() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello universe\n").unwrap();
        fs::write(
            &patch_path,
            "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
        )
        .unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: None,
            state: Some(State::Absent),
            backup: None,
            dry_run: None,
            strip: None,
        };

        let result = patch(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world\n");
    }

    #[test]
    fn test_patch_with_strip() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello world\n").unwrap();
        fs::write(
            &patch_path,
            "--- a/test.txt\n+++ b/test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
        )
        .unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: None,
            state: None,
            backup: None,
            dry_run: None,
            strip: Some(1),
        };

        let result = patch(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello universe\n");
    }

    #[test]
    fn test_patch_already_applied() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello universe\n").unwrap();
        fs::write(
            &patch_path,
            "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-hello world\n+hello universe\n",
        )
        .unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: None,
            state: None,
            backup: None,
            dry_run: None,
            strip: None,
        };

        let result = patch(params, false);
        assert!(result.is_err() || !result.unwrap().changed);
    }

    #[test]
    fn test_patch_basedir_not_found() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let patch_path = dir.path().join("test.patch");

        fs::write(&file_path, "hello world\n").unwrap();
        fs::write(&patch_path, "patch content\n").unwrap();

        let params = Params {
            src: patch_path.to_str().unwrap().to_owned(),
            dest: file_path.to_str().unwrap().to_owned(),
            basedir: Some("/nonexistent/dir".to_owned()),
            state: None,
            backup: None,
            dry_run: None,
            strip: None,
        };

        let result = patch(params, false);
        assert!(result.is_err());
    }
}
