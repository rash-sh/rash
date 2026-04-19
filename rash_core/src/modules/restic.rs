/// ANCHOR: module
/// # restic
///
/// Manage Restic backups with support for multiple backends (local, S3, B2, REST, etc.).
/// Restic is a modern, fast, secure backup program with encryption, deduplication,
/// and cloud storage support.
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
/// - name: Initialize a restic repository
///   restic:
///     repository: /mnt/backup
///     password: "{{ vault.restic_password }}"
///     state: init
///
/// - name: Backup files to local repository
///   restic:
///     repository: /mnt/backup
///     password: "{{ vault.restic_password }}"
///     state: backup
///     path:
///       - /etc
///       - /home
///     tag:
///       - daily
///       - important
///
/// - name: Backup to S3 with retention policy
///   restic:
///     repository: "s3:https://s3.amazonaws.com/my-bucket/backups"
///     password: "{{ vault.restic_password }}"
///     state: backup
///     path:
///       - /data
///     tag:
///       - s3-backup
///     keep_daily: 7
///     keep_weekly: 4
///     keep_monthly: 6
///
/// - name: Check repository integrity
///   restic:
///     repository: /mnt/backup
///     password: "{{ vault.restic_password }}"
///     state: check
///
/// - name: Restore latest snapshot
///   restic:
///     repository: /mnt/backup
///     password: "{{ vault.restic_password }}"
///     state: restore
///     restore_path: /tmp/restore
///     tag: latest
///
/// - name: Forget old snapshots with retention policy
///   restic:
///     repository: /mnt/backup
///     password: "{{ vault.restic_password }}"
///     state: forget
///     keep_daily: 7
///     keep_weekly: 4
///     keep_monthly: 6
///
/// - name: Prune unused data
///   restic:
///     repository: /mnt/backup
///     password: "{{ vault.restic_password }}"
///     state: prune
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
    Init,
    Backup,
    Check,
    Restore,
    Prune,
    Forget,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Init => write!(f, "init"),
            State::Backup => write!(f, "backup"),
            State::Check => write!(f, "check"),
            State::Restore => write!(f, "restore"),
            State::Prune => write!(f, "prune"),
            State::Forget => write!(f, "forget"),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Restic repository path or URL.
    /// Supports local paths, S3 (s3:bucket/path), B2 (b2:bucket:path),
    /// REST (rest:http://...), SFTP (sftp:user@host:/path), etc.
    pub repository: String,
    /// Repository password for encryption/decryption.
    pub password: String,
    /// Action to perform: init, backup, check, restore, prune, or forget.
    #[serde(default = "default_state")]
    pub state: State,
    /// Path(s) to backup. Required for state=backup.
    pub path: Option<Vec<String>>,
    /// Restore destination directory. Required for state=restore.
    pub restore_path: Option<String>,
    /// Tag(s) for the backup. Used with backup, restore, and forget.
    pub tag: Option<Vec<String>>,
    /// Retain daily snapshots.
    pub keep_daily: Option<u32>,
    /// Retain weekly snapshots.
    pub keep_weekly: Option<u32>,
    /// Retain monthly snapshots.
    pub keep_monthly: Option<u32>,
    /// Retain yearly snapshots.
    pub keep_yearly: Option<u32>,
    /// Retain the n most recent snapshots.
    pub keep_last: Option<u32>,
    /// Exclude pattern(s).
    pub exclude: Option<Vec<String>>,
    /// Include pattern(s).
    pub include: Option<Vec<String>>,
    /// Additional restic options.
    pub restic_opts: Option<Vec<String>>,
    /// Environment variables for restic (e.g., AWS_ACCESS_KEY_ID).
    pub environment: Option<Vec<String>>,
}

fn default_state() -> State {
    State::Backup
}

fn check_restic_available() -> Result<()> {
    let output = Command::new("restic")
        .arg("version")
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("restic not found: {}", e),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            "restic version check failed",
        ));
    }
    Ok(())
}

fn validate_params(params: &Params) -> Result<()> {
    match params.state {
        State::Backup if params.path.as_ref().is_none_or(|p| p.is_empty()) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "state 'backup' requires 'path' parameter",
            ));
        }
        State::Restore if params.restore_path.is_none() => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "state 'restore' requires 'restore_path' parameter",
            ));
        }
        _ => {}
    }

    if matches!(params.state, State::Forget) {
        let has_retention = params.keep_daily.is_some()
            || params.keep_weekly.is_some()
            || params.keep_monthly.is_some()
            || params.keep_yearly.is_some()
            || params.keep_last.is_some();
        if !has_retention {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "state 'forget' requires at least one retention policy \
                 (keep_daily, keep_weekly, keep_monthly, keep_yearly, or keep_last)",
            ));
        }
    }

    Ok(())
}

fn build_restic_env(params: &Params) -> Vec<(String, String)> {
    let mut env = vec![("RESTIC_REPOSITORY".to_string(), params.repository.clone())];
    env.push(("RESTIC_PASSWORD".to_string(), params.password.clone()));

    if let Some(ref env_vars) = params.environment {
        for pair in env_vars {
            if let Some((key, val)) = pair.split_once('=') {
                env.push((key.to_string(), val.to_string()));
            }
        }
    }

    env
}

fn build_restic_args(params: &Params, check_mode: bool) -> Vec<String> {
    let mut args = Vec::new();

    match &params.state {
        State::Init => {
            args.push("init".to_string());
        }
        State::Backup => {
            args.push("backup".to_string());
            if let Some(ref tags) = params.tag {
                for t in tags {
                    args.push("--tag".to_string());
                    args.push(t.clone());
                }
            }
            if let Some(ref excludes) = params.exclude {
                for e in excludes {
                    args.push("--exclude".to_string());
                    args.push(e.clone());
                }
            }
            if let Some(ref includes) = params.include {
                for i in includes {
                    args.push("--include".to_string());
                    args.push(i.clone());
                }
            }
            if check_mode {
                args.push("--dry-run".to_string());
            }
            if let Some(ref opts) = params.restic_opts {
                for opt in opts {
                    args.push(opt.clone());
                }
            }
            if let Some(ref paths) = params.path {
                for p in paths {
                    args.push(p.clone());
                }
            }
        }
        State::Check => {
            args.push("check".to_string());
            if let Some(ref opts) = params.restic_opts {
                for opt in opts {
                    args.push(opt.clone());
                }
            }
        }
        State::Restore => {
            args.push("restore".to_string());
            let target_tag = params
                .tag
                .as_ref()
                .and_then(|t| t.first().cloned())
                .unwrap_or_else(|| "latest".to_string());
            args.push(target_tag);
            if let Some(ref restore_path) = params.restore_path {
                args.push("--target".to_string());
                args.push(restore_path.clone());
            }
            if let Some(ref excludes) = params.exclude {
                for e in excludes {
                    args.push("--exclude".to_string());
                    args.push(e.clone());
                }
            }
            if let Some(ref includes) = params.include {
                for i in includes {
                    args.push("--include".to_string());
                    args.push(i.clone());
                }
            }
            if check_mode {
                args.push("--dry-run".to_string());
            }
            if let Some(ref opts) = params.restic_opts {
                for opt in opts {
                    args.push(opt.clone());
                }
            }
        }
        State::Prune => {
            args.push("prune".to_string());
            if let Some(ref opts) = params.restic_opts {
                for opt in opts {
                    args.push(opt.clone());
                }
            }
        }
        State::Forget => {
            args.push("forget".to_string());
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
            if let Some(ref tags) = params.tag {
                for t in tags {
                    args.push("--tag".to_string());
                    args.push(t.clone());
                }
            }
            if check_mode {
                args.push("--dry-run".to_string());
            }
            if let Some(ref opts) = params.restic_opts {
                for opt in opts {
                    args.push(opt.clone());
                }
            }
        }
    }

    args
}

fn run_restic(params: Params, check_mode: bool) -> Result<(ModuleResult, Option<Value>)> {
    trace!("params: {params:?}");

    validate_params(&params)?;
    check_restic_available()?;

    let args = build_restic_args(&params, check_mode);
    trace!("restic args: {:?}", args);

    let env = build_restic_env(&params);
    trace!(
        "restic env keys: {:?}",
        env.iter().map(|(k, _)| k).collect::<Vec<_>>()
    );

    let mut cmd = Command::new("restic");
    cmd.args(&args);
    for (key, val) in &env {
        cmd.env(key, val);
    }

    let output = cmd
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    trace!("restic output: {:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("restic failed: {}", stderr),
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
        "cmd": format!("restic {}", args.join(" ")),
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
pub struct Restic;

impl Module for Restic {
    fn get_name(&self) -> &str {
        "restic"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        run_restic(params, check_mode)
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
            repository: "/mnt/backup".to_string(),
            password: "secret123".to_string(),
            state: State::Init,
            path: None,
            restore_path: None,
            tag: None,
            keep_daily: None,
            keep_weekly: None,
            keep_monthly: None,
            keep_yearly: None,
            keep_last: None,
            exclude: None,
            include: None,
            restic_opts: None,
            environment: None,
        }
    }

    #[test]
    fn test_parse_params_init() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository: /mnt/backup
            password: secret123
            state: init
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repository, "/mnt/backup");
        assert_eq!(params.password, "secret123");
        assert_eq!(params.state, State::Init);
    }

    #[test]
    fn test_parse_params_backup() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository: /mnt/backup
            password: secret123
            state: backup
            path:
              - /etc
              - /home
            tag:
              - daily
              - important
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Backup);
        assert_eq!(
            params.path,
            Some(vec!["/etc".to_string(), "/home".to_string()])
        );
        assert_eq!(
            params.tag,
            Some(vec!["daily".to_string(), "important".to_string()])
        );
    }

    #[test]
    fn test_parse_params_restore() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository: /mnt/backup
            password: secret123
            state: restore
            restore_path: /tmp/restore
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Restore);
        assert_eq!(params.restore_path, Some("/tmp/restore".to_string()));
    }

    #[test]
    fn test_parse_params_forget_with_retention() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository: /mnt/backup
            password: secret123
            state: forget
            keep_daily: 7
            keep_weekly: 4
            keep_monthly: 6
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Forget);
        assert_eq!(params.keep_daily, Some(7));
        assert_eq!(params.keep_weekly, Some(4));
        assert_eq!(params.keep_monthly, Some(6));
    }

    #[test]
    fn test_parse_params_s3_repository() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository: "s3:https://s3.amazonaws.com/my-bucket/backups"
            password: secret123
            state: backup
            path:
              - /data
            environment:
              - "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE"
              - "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.repository,
            "s3:https://s3.amazonaws.com/my-bucket/backups"
        );
        assert_eq!(
            params.environment,
            Some(vec![
                "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE".to_string(),
                "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            ])
        );
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository: /mnt/backup
            password: secret123
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Backup);
    }

    #[test]
    fn test_parse_params_missing_repository() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            password: secret123
            state: init
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_password() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository: /mnt/backup
            state: init
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
            repository: /mnt/backup
            password: secret123
            state: init
            random: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_backup_without_path() {
        let params = Params {
            state: State::Backup,
            ..test_params()
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_backup_with_empty_path() {
        let params = Params {
            state: State::Backup,
            path: Some(vec![]),
            ..test_params()
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_restore_without_restore_path() {
        let params = Params {
            state: State::Restore,
            ..test_params()
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_forget_without_retention() {
        let params = Params {
            state: State::Forget,
            ..test_params()
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_init_valid() {
        assert!(validate_params(&test_params()).is_ok());
    }

    #[test]
    fn test_validate_params_backup_valid() {
        let params = Params {
            state: State::Backup,
            path: Some(vec!["/etc".to_string(), "/home".to_string()]),
            tag: Some(vec!["daily".to_string()]),
            ..test_params()
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_build_restic_args_init() {
        let args = build_restic_args(&test_params(), false);
        assert_eq!(args, vec!["init"]);
    }

    #[test]
    fn test_build_restic_args_backup_with_tags() {
        let params = Params {
            state: State::Backup,
            path: Some(vec!["/etc".to_string(), "/home".to_string()]),
            tag: Some(vec!["daily".to_string(), "important".to_string()]),
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert_eq!(
            args,
            vec![
                "backup",
                "--tag",
                "daily",
                "--tag",
                "important",
                "/etc",
                "/home"
            ]
        );
    }

    #[test]
    fn test_build_restic_args_backup_check_mode() {
        let params = Params {
            state: State::Backup,
            path: Some(vec!["/data".to_string()]),
            ..test_params()
        };
        let args = build_restic_args(&params, true);
        assert!(args.contains(&"--dry-run".to_string()));
    }

    #[test]
    fn test_build_restic_args_restore() {
        let params = Params {
            state: State::Restore,
            restore_path: Some("/tmp/restore".to_string()),
            tag: Some(vec!["latest".to_string()]),
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert_eq!(args, vec!["restore", "latest", "--target", "/tmp/restore"]);
    }

    #[test]
    fn test_build_restic_args_restore_default_latest() {
        let params = Params {
            state: State::Restore,
            restore_path: Some("/tmp/restore".to_string()),
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert!(args.contains(&"latest".to_string()));
    }

    #[test]
    fn test_build_restic_args_forget_with_retention() {
        let params = Params {
            state: State::Forget,
            keep_daily: Some(7),
            keep_weekly: Some(4),
            keep_monthly: Some(6),
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert!(args.contains(&"--keep-daily".to_string()));
        assert!(args.contains(&"7".to_string()));
        assert!(args.contains(&"--keep-weekly".to_string()));
        assert!(args.contains(&"4".to_string()));
        assert!(args.contains(&"--keep-monthly".to_string()));
        assert!(args.contains(&"6".to_string()));
    }

    #[test]
    fn test_build_restic_args_prune() {
        let params = Params {
            state: State::Prune,
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert_eq!(args, vec!["prune"]);
    }

    #[test]
    fn test_build_restic_args_check() {
        let params = Params {
            state: State::Check,
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert_eq!(args, vec!["check"]);
    }

    #[test]
    fn test_build_restic_args_with_excludes() {
        let params = Params {
            state: State::Backup,
            path: Some(vec!["/data".to_string()]),
            exclude: Some(vec!["*.tmp".to_string(), "*.cache".to_string()]),
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert!(args.contains(&"--exclude".to_string()));
        assert!(args.contains(&"*.tmp".to_string()));
        assert!(args.contains(&"*.cache".to_string()));
    }

    #[test]
    fn test_build_restic_args_with_restic_opts() {
        let params = Params {
            state: State::Backup,
            path: Some(vec!["/data".to_string()]),
            restic_opts: Some(vec!["--compression".to_string(), "max".to_string()]),
            ..test_params()
        };
        let args = build_restic_args(&params, false);
        assert!(args.contains(&"--compression".to_string()));
        assert!(args.contains(&"max".to_string()));
    }

    #[test]
    fn test_build_restic_env() {
        let params = Params {
            environment: Some(vec![
                "AWS_ACCESS_KEY_ID=AKIAEXAMPLE".to_string(),
                "AWS_SECRET_ACCESS_KEY=secret".to_string(),
            ]),
            ..test_params()
        };
        let env = build_restic_env(&params);
        assert!(
            env.iter()
                .any(|(k, v)| k == "RESTIC_REPOSITORY" && v == "/mnt/backup")
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "RESTIC_PASSWORD" && v == "secret123")
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "AWS_ACCESS_KEY_ID" && v == "AKIAEXAMPLE")
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "AWS_SECRET_ACCESS_KEY" && v == "secret")
        );
    }

    #[test]
    fn test_build_restic_env_invalid_pair_skipped() {
        let params = Params {
            environment: Some(vec!["INVALID_NO_EQUALS".to_string()]),
            ..test_params()
        };
        let env = build_restic_env(&params);
        assert!(!env.iter().any(|(k, _)| k == "INVALID_NO_EQUALS"));
    }

    #[test]
    fn test_state_display() {
        assert_eq!(State::Init.to_string(), "init");
        assert_eq!(State::Backup.to_string(), "backup");
        assert_eq!(State::Check.to_string(), "check");
        assert_eq!(State::Restore.to_string(), "restore");
        assert_eq!(State::Prune.to_string(), "prune");
        assert_eq!(State::Forget.to_string(), "forget");
    }
}
