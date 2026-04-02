/// ANCHOR: module
/// # rclone
///
/// Sync files and directories to/from cloud storage providers using rclone.
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
/// - name: Sync local files to S3
///   rclone:
///     command: sync
///     source: /data/backup
///     dest: s3:my-bucket/backup
///
/// - name: Copy files from Dropbox to local
///   rclone:
///     command: copy
///     source: dropbox:Documents
///     dest: /home/user/Documents
///
/// - name: Sync with filters
///   rclone:
///     command: sync
///     source: /var/log/app
///     dest: s3:logs-bucket/app-logs
///     filter:
///       - "+ *.log"
///       - "- *"
///
/// - name: Dry run to see what would change
///   rclone:
///     command: sync
///     source: /data
///     dest: gcs:my-bucket/data
///     dry_run: true
///
/// - name: Use custom config file
///   rclone:
///     command: copy
///     source: local:files
///     dest: s3:bucket/files
///     config: /etc/rclone/rclone.conf
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

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Rclone command to execute.
    /// Valid values: sync, copy, move, delete, purge, mkdir, rmdir, check, ls, lsd.
    pub command: String,
    /// Source remote:path or local path.
    pub source: String,
    /// Destination remote:path or local path.
    pub dest: Option<String>,
    /// Path to rclone config file.
    pub config: Option<String>,
    /// Create the remote if it doesn't exist.
    #[serde(default)]
    pub create_remote: bool,
    /// Remote type for create_remote (s3, gcs, dropbox, etc.).
    pub remote_type: Option<String>,
    /// List of filter patterns.
    pub filter: Option<Vec<String>>,
    /// Dry run mode - show what would be transferred without making changes.
    #[serde(default)]
    pub dry_run: bool,
    /// Skip files that match pattern.
    pub exclude: Option<Vec<String>>,
    /// Include files that match pattern.
    pub include: Option<Vec<String>>,
    /// Maximum number of times to retry failed operations.
    pub retries: Option<u32>,
    /// Reduce verbosity in output.
    #[serde(default)]
    pub quiet: bool,
    /// Additional rclone options.
    pub rclone_opts: Option<Vec<String>>,
}

fn check_rclone_available() -> Result<()> {
    let output = Command::new("rclone")
        .arg("version")
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("rclone not found: {}", e),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            "rclone version check failed",
        ));
    }
    Ok(())
}

fn build_rclone_args(params: &Params, dry_run: bool) -> Vec<String> {
    let mut args = vec![params.command.clone()];

    if let Some(ref config) = params.config {
        args.push("--config".to_string());
        args.push(config.clone());
    }

    if dry_run || params.dry_run {
        args.push("--dry-run".to_string());
    }

    if params.quiet {
        args.push("--quiet".to_string());
    }

    if let Some(retries) = params.retries {
        args.push("--retries".to_string());
        args.push(retries.to_string());
    }

    if let Some(ref filters) = params.filter {
        for f in filters {
            args.push("--filter".to_string());
            args.push(f.clone());
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

    if let Some(ref opts) = params.rclone_opts {
        for opt in opts {
            args.push(opt.clone());
        }
    }

    args.push(params.source.clone());

    if let Some(ref dest) = params.dest {
        args.push(dest.clone());
    }

    args
}

fn needs_destination(command: &str) -> bool {
    matches!(
        command,
        "sync" | "copy" | "move" | "bisync" | "check" | "cryptcheck"
    )
}

fn validate_params(params: &Params) -> Result<()> {
    let valid_commands = [
        "sync", "copy", "move", "delete", "purge", "mkdir", "rmdir", "check", "ls", "lsd", "lsf",
        "lsjson", "lsl", "size", "tree", "cat", "rcat", "serve", "config",
    ];

    if !valid_commands.contains(&params.command.as_str()) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid command '{}'. Valid commands: {}",
                params.command,
                valid_commands.join(", ")
            ),
        ));
    }

    if needs_destination(&params.command) && params.dest.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Command '{}' requires 'dest' parameter", params.command),
        ));
    }

    if params.create_remote && params.remote_type.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "create_remote requires remote_type to be specified",
        ));
    }

    Ok(())
}

fn run_rclone(params: Params, check_mode: bool) -> Result<(ModuleResult, Option<Value>)> {
    trace!("params: {params:?}");

    validate_params(&params)?;
    check_rclone_available()?;

    let args = build_rclone_args(&params, check_mode);
    trace!("rclone args: {:?}", args);

    let output = Command::new("rclone")
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    trace!("rclone output: {:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("rclone failed: {}", stderr),
        ));
    }

    let module_output = if stdout.is_empty() && stderr.is_empty() {
        None
    } else if !stdout.is_empty() {
        Some(stdout.clone())
    } else {
        Some(stderr.clone())
    };

    let changed = !params.dry_run && !check_mode;

    let extra = Some(value::to_value(json!({
        "rc": output.status.code(),
        "stdout": stdout,
        "stderr": stderr,
        "cmd": format!("rclone {}", args.join(" ")),
        "source": params.source,
        "dest": params.dest,
        "command": params.command,
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
pub struct Rclone;

impl Module for Rclone {
    fn get_name(&self) -> &str {
        "rclone"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        run_rclone(params, check_mode)
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
    fn test_parse_params_sync() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: sync
            source: /data/backup
            dest: s3:my-bucket/backup
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                command: "sync".to_string(),
                source: "/data/backup".to_string(),
                dest: Some("s3:my-bucket/backup".to_string()),
                config: None,
                create_remote: false,
                remote_type: None,
                filter: None,
                dry_run: false,
                exclude: None,
                include: None,
                retries: None,
                quiet: false,
                rclone_opts: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: copy
            source: local:files
            dest: s3:bucket/files
            config: /etc/rclone/rclone.conf
            filter:
              - "+ *.log"
              - "- *"
            dry_run: true
            retries: 3
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, "copy");
        assert_eq!(params.source, "local:files");
        assert_eq!(params.dest, Some("s3:bucket/files".to_string()));
        assert_eq!(params.config, Some("/etc/rclone/rclone.conf".to_string()));
        assert_eq!(
            params.filter,
            Some(vec!["+ *.log".to_string(), "- *".to_string()])
        );
        assert!(params.dry_run);
        assert_eq!(params.retries, Some(3));
    }

    #[test]
    fn test_parse_params_missing_command() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: /data
            dest: s3:bucket
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_source() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: sync
            dest: s3:bucket
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
            command: sync
            source: /data
            dest: s3:bucket
            random: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_build_rclone_args_basic() {
        let params = Params {
            command: "sync".to_string(),
            source: "/data/backup".to_string(),
            dest: Some("s3:my-bucket/backup".to_string()),
            config: None,
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let args = build_rclone_args(&params, false);
        assert_eq!(args, vec!["sync", "/data/backup", "s3:my-bucket/backup"]);
    }

    #[test]
    fn test_build_rclone_args_with_config() {
        let params = Params {
            command: "copy".to_string(),
            source: "local:files".to_string(),
            dest: Some("s3:bucket/files".to_string()),
            config: Some("/etc/rclone/rclone.conf".to_string()),
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let args = build_rclone_args(&params, false);
        assert_eq!(
            args,
            vec![
                "copy",
                "--config",
                "/etc/rclone/rclone.conf",
                "local:files",
                "s3:bucket/files"
            ]
        );
    }

    #[test]
    fn test_build_rclone_args_with_dry_run() {
        let params = Params {
            command: "sync".to_string(),
            source: "/data".to_string(),
            dest: Some("gcs:bucket/data".to_string()),
            config: None,
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: true,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let args = build_rclone_args(&params, false);
        assert!(args.contains(&"--dry-run".to_string()));
    }

    #[test]
    fn test_build_rclone_args_with_filters() {
        let params = Params {
            command: "sync".to_string(),
            source: "/var/log".to_string(),
            dest: Some("s3:logs".to_string()),
            config: None,
            create_remote: false,
            remote_type: None,
            filter: Some(vec!["+ *.log".to_string(), "- *".to_string()]),
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let args = build_rclone_args(&params, false);
        assert!(args.contains(&"--filter".to_string()));
        assert!(args.contains(&"+ *.log".to_string()));
        assert!(args.contains(&"- *".to_string()));
    }

    #[test]
    fn test_build_rclone_args_no_dest() {
        let params = Params {
            command: "ls".to_string(),
            source: "s3:bucket".to_string(),
            dest: None,
            config: None,
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let args = build_rclone_args(&params, false);
        assert_eq!(args, vec!["ls", "s3:bucket"]);
    }

    #[test]
    fn test_needs_destination() {
        assert!(needs_destination("sync"));
        assert!(needs_destination("copy"));
        assert!(needs_destination("move"));
        assert!(needs_destination("check"));
        assert!(!needs_destination("ls"));
        assert!(!needs_destination("delete"));
        assert!(!needs_destination("purge"));
        assert!(!needs_destination("mkdir"));
    }

    #[test]
    fn test_validate_params_valid() {
        let params = Params {
            command: "sync".to_string(),
            source: "/data".to_string(),
            dest: Some("s3:bucket".to_string()),
            config: None,
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_invalid_command() {
        let params = Params {
            command: "invalid".to_string(),
            source: "/data".to_string(),
            dest: Some("s3:bucket".to_string()),
            config: None,
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_missing_dest_for_sync() {
        let params = Params {
            command: "sync".to_string(),
            source: "/data".to_string(),
            dest: None,
            config: None,
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_ls_no_dest_needed() {
        let params = Params {
            command: "ls".to_string(),
            source: "s3:bucket".to_string(),
            dest: None,
            config: None,
            create_remote: false,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_create_remote_without_type() {
        let params = Params {
            command: "sync".to_string(),
            source: "/data".to_string(),
            dest: Some("s3:bucket".to_string()),
            config: None,
            create_remote: true,
            remote_type: None,
            filter: None,
            dry_run: false,
            exclude: None,
            include: None,
            retries: None,
            quiet: false,
            rclone_opts: None,
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
