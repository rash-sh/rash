/// ANCHOR: module
/// # borgmatic
///
/// Manage Borg/Borgmatic backups with support for create, extract, prune, and check operations.
/// Borg is a deduplicating archiver with compression and authenticated encryption.
/// Borgmatic is a wrapper that simplifies backup configuration and automation.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: partial
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Create a backup using borgmatic config
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     state: create
///
/// - name: Create backup with custom repository and passphrase
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     repository: /mnt/backups/my_repo
///     passphrase: "{{ vault.borg_passphrase }}"
///     state: create
///     compression: zstd
///
/// - name: Create backup with exclusion patterns
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     state: create
///     exclude_patterns:
///       - "*.tmp"
///       - "/home/*/.cache"
///
/// - name: Extract archive to a target directory
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     passphrase: "{{ vault.borg_passphrase }}"
///     state: extract
///     archive: my-backup-2024-01-15
///     extract_path: /tmp/restore
///
/// - name: Prune old archives with retention policy
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     state: prune
///     keep_daily: 7
///     keep_weekly: 4
///     keep_monthly: 6
///
/// - name: Check repository integrity
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     passphrase: "{{ vault.borg_passphrase }}"
///     state: check
///
/// - name: List archives in repository
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     state: list
///
/// - name: Run create and prune together
///   borgmatic:
///     config_path: /etc/borgmatic.d/my_backup.yaml
///     state: create
///     keep_daily: 7
///     keep_weekly: 4
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Create,
    Extract,
    Prune,
    Check,
    List,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Create => write!(f, "create"),
            State::Extract => write!(f, "extract"),
            State::Prune => write!(f, "prune"),
            State::Check => write!(f, "check"),
            State::List => write!(f, "list"),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the borgmatic configuration file.
    pub config_path: String,
    /// Borg repository path. Overrides the repository in config.
    pub repository: Option<String>,
    /// Action to perform: create, extract, prune, check, or list.
    #[serde(default = "default_state")]
    pub state: State,
    /// Archive name pattern for extract or list operations.
    pub archive: Option<String>,
    /// Repository passphrase for encryption/decryption.
    pub passphrase: Option<String>,
    /// Compression algorithm (e.g., none, lz4, zstd, zstd,1-22, zlib, lzma).
    pub compression: Option<String>,
    /// File patterns to exclude from backup.
    pub exclude_patterns: Option<Vec<String>>,
    /// Directory to extract files into. Required for state=extract.
    pub extract_path: Option<String>,
    /// Retain daily archives.
    pub keep_daily: Option<u32>,
    /// Retain weekly archives.
    pub keep_weekly: Option<u32>,
    /// Retain monthly archives.
    pub keep_monthly: Option<u32>,
    /// Retain yearly archives.
    pub keep_yearly: Option<u32>,
    /// Retain the n most recent archives.
    pub keep_last: Option<u32>,
    /// Additional borgmatic options.
    pub borgmatic_opts: Option<Vec<String>>,
    /// Environment variables for borgmatic (e.g., BORG_REMOTE_PATH).
    pub environment: Option<Vec<String>>,
}

fn default_state() -> State {
    State::Create
}

fn check_borgmatic_available() -> Result<()> {
    let output = Command::new("borgmatic")
        .arg("--version")
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("borgmatic not found: {}", e),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            "borgmatic version check failed",
        ));
    }
    Ok(())
}

fn validate_params(params: &Params) -> Result<()> {
    match params.state {
        State::Extract if params.archive.is_none() => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "state 'extract' requires 'archive' parameter",
            ));
        }
        _ => {}
    }

    Ok(())
}

fn build_borgmatic_env(params: &Params) -> Vec<(String, String)> {
    let mut env = Vec::new();

    if let Some(ref passphrase) = params.passphrase {
        env.push(("BORG_PASSPHRASE".to_string(), passphrase.clone()));
    }

    if let Some(ref env_vars) = params.environment {
        for pair in env_vars {
            if let Some((key, val)) = pair.split_once('=') {
                env.push((key.to_string(), val.to_string()));
            }
        }
    }

    env
}

fn build_borgmatic_args(params: &Params, check_mode: bool) -> Vec<String> {
    let mut args = Vec::new();

    args.push("--config".to_string());
    args.push(params.config_path.clone());

    if let Some(ref repository) = params.repository {
        args.push("--repository".to_string());
        args.push(repository.clone());
    }

    match &params.state {
        State::Create => {
            args.push("create".to_string());
            if let Some(ref compression) = params.compression {
                args.push("--compression".to_string());
                args.push(compression.clone());
            }
            if let Some(ref excludes) = params.exclude_patterns {
                for e in excludes {
                    args.push("--exclude".to_string());
                    args.push(e.clone());
                }
            }
            if check_mode {
                args.push("--dry-run".to_string());
            }
            if params.keep_daily.is_some()
                || params.keep_weekly.is_some()
                || params.keep_monthly.is_some()
                || params.keep_yearly.is_some()
                || params.keep_last.is_some()
            {
                args.push("--stats".to_string());
            }
        }
        State::Extract => {
            args.push("extract".to_string());
            if let Some(ref archive) = params.archive {
                args.push("--archive".to_string());
                args.push(archive.clone());
            }
            if let Some(ref extract_path) = params.extract_path {
                args.push("--destination".to_string());
                args.push(extract_path.clone());
            }
            if check_mode {
                args.push("--dry-run".to_string());
            }
        }
        State::Prune => {
            args.push("prune".to_string());
            if let Some(keep_daily) = params.keep_daily {
                args.push("--keep-daily".to_string());
                args.push(keep_daily.to_string());
            }
            if let Some(keep_weekly) = params.keep_weekly {
                args.push("--keep-weekly".to_string());
                args.push(keep_weekly.to_string());
            }
            if let Some(keep_monthly) = params.keep_monthly {
                args.push("--keep-monthly".to_string());
                args.push(keep_monthly.to_string());
            }
            if let Some(keep_yearly) = params.keep_yearly {
                args.push("--keep-yearly".to_string());
                args.push(keep_yearly.to_string());
            }
            if let Some(keep_last) = params.keep_last {
                args.push("--keep-last".to_string());
                args.push(keep_last.to_string());
            }
            if check_mode {
                args.push("--dry-run".to_string());
            }
            args.push("--stats".to_string());
        }
        State::Check => {
            args.push("check".to_string());
            if check_mode {
                args.push("--dry-run".to_string());
            }
        }
        State::List => {
            args.push("list".to_string());
            if let Some(ref archive) = params.archive {
                args.push("--archive".to_string());
                args.push(archive.clone());
            }
        }
    }

    if let Some(ref opts) = params.borgmatic_opts {
        for opt in opts {
            args.push(opt.clone());
        }
    }

    args
}

fn run_borgmatic(params: Params, check_mode: bool) -> Result<(ModuleResult, Option<Value>)> {
    trace!("params: {params:?}");

    validate_params(&params)?;
    check_borgmatic_available()?;

    let args = build_borgmatic_args(&params, check_mode);
    trace!("borgmatic args: {:?}", args);

    let env = build_borgmatic_env(&params);
    trace!(
        "borgmatic env keys: {:?}",
        env.iter().map(|(k, _)| k).collect::<Vec<_>>()
    );

    let mut cmd = Command::new("borgmatic");
    cmd.args(&args);
    for (key, val) in &env {
        cmd.env(key, val);
    }

    let output = cmd
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    trace!("borgmatic output: {:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("borgmatic failed: {}", stderr),
        ));
    }

    let module_output = if stdout.is_empty() && stderr.is_empty() {
        None
    } else if !stdout.is_empty() {
        Some(stdout.clone())
    } else {
        Some(stderr.clone())
    };

    let changed = !check_mode;

    let extra = Some(value::to_value(json!({
        "rc": output.status.code(),
        "stdout": stdout,
        "stderr": stderr,
        "cmd": format!("borgmatic {}", args.join(" ")),
        "config_path": params.config_path,
        "repository": params.repository,
        "state": params.state.to_string(),
    }))?);

    Ok((
        ModuleResult {
            changed,
            output: module_output,
            extra,
        },
        None,
    ))
}

#[derive(Debug)]
pub struct Borgmatic;

impl Module for Borgmatic {
    fn get_name(&self) -> &str {
        "borgmatic"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        run_borgmatic(params, check_mode)
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> Params {
        Params {
            config_path: "/etc/borgmatic.d/test.yaml".to_string(),
            repository: None,
            state: State::Create,
            archive: None,
            passphrase: None,
            compression: None,
            exclude_patterns: None,
            extract_path: None,
            keep_daily: None,
            keep_weekly: None,
            keep_monthly: None,
            keep_yearly: None,
            keep_last: None,
            borgmatic_opts: None,
            environment: None,
        }
    }

    #[test]
    fn test_parse_params_create() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            state: create
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.config_path, "/etc/borgmatic.d/test.yaml");
        assert_eq!(params.state, State::Create);
    }

    #[test]
    fn test_parse_params_with_repository() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            repository: /mnt/backups/my_repo
            passphrase: secret123
            state: create
            compression: zstd
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repository, Some("/mnt/backups/my_repo".to_string()));
        assert_eq!(params.passphrase, Some("secret123".to_string()));
        assert_eq!(params.compression, Some("zstd".to_string()));
    }

    #[test]
    fn test_parse_params_extract() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            repository: /mnt/backups/my_repo
            state: extract
            archive: my-backup-2024-01-15
            extract_path: /tmp/restore
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Extract);
        assert_eq!(params.archive, Some("my-backup-2024-01-15".to_string()));
        assert_eq!(params.extract_path, Some("/tmp/restore".to_string()));
    }

    #[test]
    fn test_parse_params_prune_with_retention() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            state: prune
            keep_daily: 7
            keep_weekly: 4
            keep_monthly: 6
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Prune);
        assert_eq!(params.keep_daily, Some(7));
        assert_eq!(params.keep_weekly, Some(4));
        assert_eq!(params.keep_monthly, Some(6));
    }

    #[test]
    fn test_parse_params_check() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            passphrase: secret123
            state: check
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Check);
    }

    #[test]
    fn test_parse_params_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            state: list
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::List);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Create);
    }

    #[test]
    fn test_parse_params_missing_config_path() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: create
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            random: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_with_excludes() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            state: create
            exclude_patterns:
              - "*.tmp"
              - "/home/*/.cache"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.exclude_patterns,
            Some(vec!["*.tmp".to_string(), "/home/*/.cache".to_string()])
        );
    }

    #[test]
    fn test_parse_params_with_environment() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            config_path: /etc/borgmatic.d/test.yaml
            state: create
            environment:
              - "BORG_REMOTE_PATH=/usr/bin/borg"
              - "CUSTOM_VAR=value"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.environment,
            Some(vec![
                "BORG_REMOTE_PATH=/usr/bin/borg".to_string(),
                "CUSTOM_VAR=value".to_string(),
            ])
        );
    }

    #[test]
    fn test_validate_params_extract_without_archive() {
        let params = Params {
            state: State::Extract,
            ..test_params()
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_create_valid() {
        assert!(validate_params(&test_params()).is_ok());
    }

    #[test]
    fn test_validate_params_extract_with_archive() {
        let params = Params {
            state: State::Extract,
            archive: Some("my-archive".to_string()),
            ..test_params()
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_build_borgmatic_args_create() {
        let args = build_borgmatic_args(&test_params(), false);
        assert_eq!(args[0], "--config");
        assert_eq!(args[1], "/etc/borgmatic.d/test.yaml");
        assert_eq!(args[2], "create");
    }

    #[test]
    fn test_build_borgmatic_args_create_with_compression() {
        let params = Params {
            compression: Some("zstd".to_string()),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"--compression".to_string()));
        assert!(args.contains(&"zstd".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_create_with_excludes() {
        let params = Params {
            exclude_patterns: Some(vec!["*.tmp".to_string(), "*.cache".to_string()]),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"--exclude".to_string()));
        assert!(args.contains(&"*.tmp".to_string()));
        assert!(args.contains(&"*.cache".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_create_check_mode() {
        let params = test_params();
        let args = build_borgmatic_args(&params, true);
        assert!(args.contains(&"--dry-run".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_create_with_retention() {
        let params = Params {
            keep_daily: Some(7),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"--stats".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_extract() {
        let params = Params {
            state: State::Extract,
            repository: Some("/mnt/backups/my_repo".to_string()),
            archive: Some("my-backup-2024-01-15".to_string()),
            extract_path: Some("/tmp/restore".to_string()),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"extract".to_string()));
        assert!(args.contains(&"--archive".to_string()));
        assert!(args.contains(&"my-backup-2024-01-15".to_string()));
        assert!(!args.iter().any(|a| a.contains("::")));
        assert!(args.contains(&"--destination".to_string()));
        assert!(args.contains(&"/tmp/restore".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_prune_with_retention() {
        let params = Params {
            state: State::Prune,
            keep_daily: Some(7),
            keep_weekly: Some(4),
            keep_monthly: Some(6),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"prune".to_string()));
        assert!(args.contains(&"--keep-daily".to_string()));
        assert!(args.contains(&"7".to_string()));
        assert!(args.contains(&"--keep-weekly".to_string()));
        assert!(args.contains(&"4".to_string()));
        assert!(args.contains(&"--keep-monthly".to_string()));
        assert!(args.contains(&"6".to_string()));
        assert!(args.contains(&"--stats".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_check() {
        let params = Params {
            state: State::Check,
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"check".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_list() {
        let params = Params {
            state: State::List,
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"list".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_list_with_archive() {
        let params = Params {
            state: State::List,
            archive: Some("my-archive".to_string()),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"--archive".to_string()));
        assert!(args.contains(&"my-archive".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_with_repository_override() {
        let params = Params {
            repository: Some("/mnt/other_repo".to_string()),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"--repository".to_string()));
        assert!(args.contains(&"/mnt/other_repo".to_string()));
    }

    #[test]
    fn test_build_borgmatic_args_with_borgmatic_opts() {
        let params = Params {
            borgmatic_opts: Some(vec!["--verbose".to_string()]),
            ..test_params()
        };
        let args = build_borgmatic_args(&params, false);
        assert!(args.contains(&"--verbose".to_string()));
    }

    #[test]
    fn test_build_borgmatic_env_with_passphrase() {
        let params = Params {
            passphrase: Some("secret123".to_string()),
            ..test_params()
        };
        let env = build_borgmatic_env(&params);
        assert!(
            env.iter()
                .any(|(k, v)| k == "BORG_PASSPHRASE" && v == "secret123")
        );
    }

    #[test]
    fn test_build_borgmatic_env_without_passphrase() {
        let params = test_params();
        let env = build_borgmatic_env(&params);
        assert!(!env.iter().any(|(k, _)| k == "BORG_PASSPHRASE"));
    }

    #[test]
    fn test_build_borgmatic_env_with_environment() {
        let params = Params {
            environment: Some(vec![
                "BORG_REMOTE_PATH=/usr/bin/borg".to_string(),
                "CUSTOM_VAR=value".to_string(),
            ]),
            ..test_params()
        };
        let env = build_borgmatic_env(&params);
        assert!(
            env.iter()
                .any(|(k, v)| k == "BORG_REMOTE_PATH" && v == "/usr/bin/borg")
        );
        assert!(env.iter().any(|(k, v)| k == "CUSTOM_VAR" && v == "value"));
    }

    #[test]
    fn test_build_borgmatic_env_invalid_pair_skipped() {
        let params = Params {
            environment: Some(vec!["INVALID_NO_EQUALS".to_string()]),
            ..test_params()
        };
        let env = build_borgmatic_env(&params);
        assert!(!env.iter().any(|(k, _)| k == "INVALID_NO_EQUALS"));
    }

    #[test]
    fn test_state_display() {
        assert_eq!(State::Create.to_string(), "create");
        assert_eq!(State::Extract.to_string(), "extract");
        assert_eq!(State::Prune.to_string(), "prune");
        assert_eq!(State::Check.to_string(), "check");
        assert_eq!(State::List.to_string(), "list");
    }
}
