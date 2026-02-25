/// ANCHOR: module
/// # apt_repository
///
/// Manage APT repositories on Debian/Ubuntu systems.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Add Docker repository
///   apt_repository:
///     repo: deb https://download.docker.com/linux/ubuntu focal stable
///     state: present
///     filename: docker
///
/// - name: Add repository with custom codename
///   apt_repository:
///     repo: deb http://archive.ubuntu.com/ubuntu jammy main restricted
///     state: present
///     codename: jammy
///
/// - name: Remove old repository
///   apt_repository:
///     repo: deb http://old-repo.example.com focal main
///     state: absent
///
/// - name: Add repository without updating cache
///   apt_repository:
///     repo: deb https://example.com/repo stable main
///     update_cache: false
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::{self, diff};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{OpenOptions, read_to_string};
use std::io::prelude::*;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const SOURCES_LIST_DIR: &str = "/etc/apt/sources.list.d";
const SOURCES_LIST: &str = "/etc/apt/sources.list";

#[derive(Debug, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

fn default_true() -> Option<bool> {
    Some(true)
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Repository string in sources.list format (required).
    pub repo: String,
    /// Whether the repository should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// File mode for the sources list file (octal, e.g., "0644").
    /// **[default: `"0644"`]**
    pub mode: Option<String>,
    /// Run apt-get update after adding or removing the repository.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub update_cache: Option<bool>,
    /// Whether to validate SSL certificates when fetching the repository.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub validate_certs: Option<bool>,
    /// Custom filename for the sources list (without .list extension).
    /// If not specified, the repository will be added to the main sources.list.
    pub filename: Option<String>,
    /// Distribution codename override.
    pub codename: Option<String>,
}

fn normalize_repo_line(repo: &str, codename_override: Option<&str>) -> Result<String> {
    let mut repo = repo.trim().to_string();

    if let Some(codename) = codename_override {
        let output = Command::new("lsb_release")
            .arg("-sc")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            });

        if let Some(current_codename) = output {
            repo = repo.replace(&current_codename, codename);
        }
    }

    if !repo.ends_with('\n') {
        repo.push('\n');
    }

    Ok(repo)
}

fn get_sources_file_path(filename: Option<&str>) -> PathBuf {
    match filename {
        Some(name) => {
            let filename = if name.ends_with(".list") {
                name.to_string()
            } else {
                format!("{name}.list")
            };
            PathBuf::from(SOURCES_LIST_DIR).join(filename)
        }
        None => PathBuf::from(SOURCES_LIST),
    }
}

fn repo_exists_in_content(content: &str, repo_line: &str) -> bool {
    let normalized_repo = repo_line.trim();
    content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == normalized_repo
            || (trimmed.starts_with('#') && trimmed[1..].trim() == normalized_repo)
    })
}

fn repo_exists_in_file(path: &Path, repo_line: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let content = read_to_string(path)?;
    Ok(repo_exists_in_content(&content, repo_line))
}

fn find_repo_line_in_content(content: &str, repo_pattern: &str) -> Option<(usize, String)> {
    let normalized_pattern = repo_pattern.trim();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == normalized_pattern
            || (trimmed.starts_with('#') && trimmed[1..].trim() == normalized_pattern)
        {
            return Some((idx, line.to_string()));
        }
    }
    None
}

fn add_repo_to_file(path: &Path, repo_line: &str, mode: u32, check_mode: bool) -> Result<bool> {
    if repo_exists_in_file(path, repo_line)? {
        return Ok(false);
    }

    let original_content = if path.exists() {
        read_to_string(path)?
    } else {
        String::new()
    };

    let mut new_content = original_content.clone();
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(repo_line);

    diff(&original_content, &new_content);

    if !check_mode {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(path)?;
        file.write_all(new_content.as_bytes())?;
    }

    Ok(true)
}

fn remove_repo_from_file(path: &Path, repo_line: &str, check_mode: bool) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let original_content = read_to_string(path)?;

    let Some((line_idx, _)) = find_repo_line_in_content(&original_content, repo_line) else {
        return Ok(false);
    };

    let mut lines: Vec<String> = original_content.lines().map(|s| s.to_string()).collect();
    lines.remove(line_idx);

    let new_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    diff(&original_content, &new_content);

    if !check_mode {
        if lines.is_empty() {
            std::fs::remove_file(path)?;
        } else {
            let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
            file.write_all(new_content.as_bytes())?;
        }
    }

    Ok(true)
}

fn run_apt_get_update(validate_certs: bool) -> Result<()> {
    let mut cmd = Command::new("apt-get");
    cmd.arg("update");

    if !validate_certs {
        cmd.env("APT_CONFIG", "/dev/null");
        cmd.arg("-o");
        cmd.arg("Acquire::https::Verify-Peer=false");
        cmd.arg("-o");
        cmd.arg("Acquire::https::Verify-Host=false");
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to run apt-get update: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

pub fn apt_repository(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let mode = match params.mode {
        Some(ref mode_str) => u32::from_str_radix(mode_str, 8).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid mode '{mode_str}': {e}"),
            )
        })?,
        None => 0o644,
    };
    let update_cache = params.update_cache.unwrap_or(true);
    let validate_certs = params.validate_certs.unwrap_or(true);

    let repo_line = normalize_repo_line(&params.repo, params.codename.as_deref())?;

    let file_path = get_sources_file_path(params.filename.as_deref());

    let changed = match state {
        State::Present => {
            logger::add(std::slice::from_ref(&params.repo));
            add_repo_to_file(&file_path, &repo_line, mode, check_mode)?
        }
        State::Absent => {
            logger::remove(std::slice::from_ref(&params.repo));
            remove_repo_from_file(&file_path, &repo_line, check_mode)?
        }
    };

    if changed && update_cache && !check_mode {
        run_apt_get_update(validate_certs)?;
    }

    Ok(ModuleResult {
        changed,
        output: Some(file_path.to_string_lossy().to_string()),
        extra: Some(serde_norway::Value::String(repo_line.trim().to_string())),
    })
}

#[derive(Debug)]
pub struct AptRepository;

impl Module for AptRepository {
    fn get_name(&self) -> &str {
        "apt_repository"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            apt_repository(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
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
            repo: deb https://example.com/repo stable main
            state: present
            filename: example
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                repo: "deb https://example.com/repo stable main".to_owned(),
                state: Some(State::Present),
                mode: None,
                update_cache: Some(true),
                validate_certs: Some(true),
                filename: Some("example".to_owned()),
                codename: None,
            }
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repo: deb https://example.com/repo stable main
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repo, "deb https://example.com/repo stable main");
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.filename, None);
    }

    #[test]
    fn test_normalize_repo_line() {
        let repo = "deb https://example.com/repo stable main";
        let normalized = normalize_repo_line(repo, None).unwrap();
        assert!(normalized.ends_with('\n'));
    }

    #[test]
    fn test_get_sources_file_path_with_filename() {
        let path = get_sources_file_path(Some("docker"));
        assert_eq!(
            path.to_str().unwrap(),
            "/etc/apt/sources.list.d/docker.list"
        );
    }

    #[test]
    fn test_get_sources_file_path_with_extension() {
        let path = get_sources_file_path(Some("custom.list"));
        assert_eq!(
            path.to_str().unwrap(),
            "/etc/apt/sources.list.d/custom.list"
        );
    }

    #[test]
    fn test_get_sources_file_path_without_filename() {
        let path = get_sources_file_path(None);
        assert_eq!(path.to_str().unwrap(), "/etc/apt/sources.list");
    }

    #[test]
    fn test_repo_exists_in_content() {
        let content = "deb https://example.com/repo stable main\ndeb http://other.com focal main\n";
        assert!(repo_exists_in_content(
            content,
            "deb https://example.com/repo stable main"
        ));
        assert!(!repo_exists_in_content(
            content,
            "deb https://notpresent.com repo main"
        ));
    }

    #[test]
    fn test_repo_exists_in_content_commented() {
        let content = "# deb https://example.com/repo stable main\n";
        assert!(repo_exists_in_content(
            content,
            "deb https://example.com/repo stable main"
        ));
    }

    #[test]
    fn test_find_repo_line_in_content() {
        let content = "deb https://example.com/repo stable main\ndeb http://other.com focal main\n";
        let result = find_repo_line_in_content(content, "deb https://example.com/repo stable main");
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 0);
    }

    #[test]
    fn test_add_repo_to_file_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        let changed = add_repo_to_file(
            &file_path,
            "deb https://example.com/repo stable main\n",
            0o644,
            false,
        )
        .unwrap();

        assert!(changed);
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("deb https://example.com/repo stable main"));
    }

    #[test]
    fn test_add_repo_to_file_existing_repo() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        fs::write(&file_path, "deb https://example.com/repo stable main\n").unwrap();

        let changed = add_repo_to_file(
            &file_path,
            "deb https://example.com/repo stable main\n",
            0o644,
            false,
        )
        .unwrap();

        assert!(!changed);
    }

    #[test]
    fn test_add_repo_to_file_append() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        fs::write(&file_path, "deb http://first.com focal main\n").unwrap();

        let changed = add_repo_to_file(
            &file_path,
            "deb http://second.com jammy main\n",
            0o644,
            false,
        )
        .unwrap();

        assert!(changed);
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("first.com"));
        assert!(content.contains("second.com"));
    }

    #[test]
    fn test_remove_repo_from_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        fs::write(
            &file_path,
            "deb https://example.com/repo stable main\ndeb http://other.com focal main\n",
        )
        .unwrap();

        let changed = remove_repo_from_file(
            &file_path,
            "deb https://example.com/repo stable main\n",
            false,
        )
        .unwrap();

        assert!(changed);
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("example.com"));
        assert!(content.contains("other.com"));
    }

    #[test]
    fn test_remove_repo_from_file_not_found() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        fs::write(&file_path, "deb http://other.com focal main\n").unwrap();

        let changed = remove_repo_from_file(
            &file_path,
            "deb https://example.com/repo stable main\n",
            false,
        )
        .unwrap();

        assert!(!changed);
    }

    #[test]
    fn test_remove_repo_from_file_removes_empty_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        fs::write(&file_path, "deb https://example.com/repo stable main\n").unwrap();

        let changed = remove_repo_from_file(
            &file_path,
            "deb https://example.com/repo stable main\n",
            false,
        )
        .unwrap();

        assert!(changed);
        assert!(!file_path.exists());
    }

    #[test]
    fn test_add_repo_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        let changed = add_repo_to_file(
            &file_path,
            "deb https://example.com/repo stable main\n",
            0o644,
            true,
        )
        .unwrap();

        assert!(changed);
        assert!(!file_path.exists());
    }

    #[test]
    fn test_remove_repo_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.list");

        fs::write(
            &file_path,
            "deb https://example.com/repo stable main\ndeb http://other.com focal main\n",
        )
        .unwrap();
        let original_content = fs::read_to_string(&file_path).unwrap();

        let changed = remove_repo_from_file(
            &file_path,
            "deb https://example.com/repo stable main\n",
            true,
        )
        .unwrap();

        assert!(changed);
        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_parse_params_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repo: deb https://example.com/repo stable main
            state: absent
            mode: "0600"
            update_cache: false
            validate_certs: false
            filename: custom
            codename: jammy
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
        assert_eq!(params.mode, Some("0600".to_owned()));
        assert_eq!(params.update_cache, Some(false));
        assert_eq!(params.validate_certs, Some(false));
        assert_eq!(params.filename, Some("custom".to_owned()));
        assert_eq!(params.codename, Some("jammy".to_owned()));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repo: deb https://example.com/repo stable main
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
