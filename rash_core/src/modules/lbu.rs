/// ANCHOR: module
/// # lbu
///
/// Manage Alpine Local Backup (lbu) for diskless Alpine systems.
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
/// - name: Commit LBU changes
///   lbu:
///     state: commit
///
/// - name: Commit with custom message
///   lbu:
///     state: commit
///     message: "Added app configuration"
///
/// - name: Rollback to last committed state
///   lbu:
///     state: rollback
///
/// - name: Add files to LBU include list
///   lbu:
///     include:
///       - /etc/config/app.conf
///       - /etc/app/settings.yaml
///
/// - name: Remove files from LBU include list
///   lbu:
///     exclude:
///       - /etc/config/temp.conf
///
/// - name: Create backup package
///   lbu:
///     package: /backup/backup.apk
///
/// - name: Set backup media
///   lbu:
///     media: usb
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::{Value as YamlValue, value};
use serde_with::{OneOrMany, serde_as};
use shlex::split;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("lbu".to_owned())
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Commit,
    Rollback,
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the lbu binary to use.
    /// **[default: `"lbu"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to lbu.
    extra_args: Option<String>,
    /// LBU overlay directory path.
    path: Option<String>,
    /// File or list of files to add to LBU include list.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    include: Vec<String>,
    /// File or list of files to remove from LBU include list.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    exclude: Vec<String>,
    /// Action to perform: `commit` saves changes, `rollback` reverts to last commit.
    state: Option<State>,
    /// Commit message for lbu commit.
    message: Option<String>,
    /// Create an apk package backup at the specified path.
    package: Option<String>,
    /// Backup media type (e.g., `usb`, `floppy`).
    media: Option<String>,
    /// Enable verbose output.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    verbose: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("lbu".to_owned()),
            extra_args: None,
            path: None,
            include: Vec::new(),
            exclude: Vec::new(),
            state: None,
            message: None,
            package: None,
            media: None,
            verbose: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Lbu;

impl Module for Lbu {
    fn get_name(&self) -> &str {
        "lbu"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((lbu(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct LbuClient {
    executable: PathBuf,
    extra_args: Option<String>,
    path: Option<String>,
    verbose: bool,
    check_mode: bool,
}

impl LbuClient {
    pub fn new(
        executable: &Path,
        extra_args: Option<String>,
        path: Option<String>,
        verbose: bool,
        check_mode: bool,
    ) -> Result<Self> {
        Ok(LbuClient {
            executable: executable.to_path_buf(),
            extra_args,
            path,
            verbose,
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        if self.verbose {
            cmd.arg("-v");
        }
        if let Some(path) = &self.path {
            cmd.arg("-d").arg(path);
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        if let Some(extra_args) = &self.extra_args {
            cmd.args(split(extra_args).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid extra_args: {extra_args}"),
                )
            })?);
        };
        let output = cmd
            .output()
            .map_err(|e| Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable.display()
                ),
            ))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(output)
    }

    pub fn add(&self, files: &[String]) -> Result<bool> {
        if self.check_mode {
            return Ok(!files.is_empty());
        }

        if files.is_empty() {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("add").args(files);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn delete(&self, files: &[String]) -> Result<bool> {
        if self.check_mode {
            return Ok(!files.is_empty());
        }

        if files.is_empty() {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("delete").args(files);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn commit(&self, message: Option<&str>) -> Result<bool> {
        if self.check_mode {
            let mut cmd = self.get_cmd();
            cmd.arg("diff");
            let output = self.exec_cmd(&mut cmd, false)?;
            let has_changes = !output.stdout.is_empty();
            return Ok(has_changes);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("ci");
        if let Some(msg) = message {
            cmd.arg("-m").arg(msg);
        }
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn rollback(&self) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("revert");
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn package(&self, dest: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("package").arg(dest);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn set_media(&self, media: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("media").arg(media);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }
}

fn lbu(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = LbuClient::new(
        Path::new(&params.executable.unwrap()),
        params.extra_args.clone(),
        params.path.clone(),
        params.verbose.unwrap(),
        check_mode,
    )?;

    let mut changed = false;
    let mut actions: Vec<&str> = Vec::new();

    if !params.include.is_empty() {
        logger::add(&params.include);
        if client.add(&params.include)? {
            changed = true;
            actions.push("included");
        }
    }

    if !params.exclude.is_empty() {
        logger::remove(&params.exclude);
        if client.delete(&params.exclude)? {
            changed = true;
            actions.push("excluded");
        }
    }

    if let Some(ref media) = params.media
        && client.set_media(media)?
    {
        changed = true;
        actions.push("media_set");
    }

    if let Some(ref package) = params.package
        && client.package(package)?
    {
        changed = true;
        actions.push("packaged");
    }

    match params.state {
        Some(State::Commit) => {
            if client.commit(params.message.as_deref())? {
                changed = true;
                actions.push("committed");
            }
        }
        Some(State::Rollback) => {
            if client.rollback()? {
                changed = true;
                actions.push("rolled_back");
            }
        }
        None => {}
    }

    Ok(ModuleResult {
        changed,
        output: None,
        extra: Some(value::to_value(json!({"actions": actions}))?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_commit() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: commit
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                state: Some(State::Commit),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_rollback() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: rollback
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                state: Some(State::Rollback),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_include() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            include:
              - /etc/config/app.conf
              - /etc/app/settings.yaml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                include: vec![
                    "/etc/config/app.conf".to_owned(),
                    "/etc/app/settings.yaml".to_owned()
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_include_single() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            include: /etc/config/app.conf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                include: vec!["/etc/config/app.conf".to_owned()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_exclude() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            exclude:
              - /etc/config/temp.conf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                exclude: vec!["/etc/config/temp.conf".to_owned()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_package() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            package: /backup/backup.apk
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                package: Some("/backup/backup.apk".to_owned()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_media() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            media: usb
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                media: Some("usb".to_owned()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /sbin/lbu
            extra_args: "-p /custom/path"
            path: /overlay
            include:
              - /etc/config/app.conf
            exclude:
              - /etc/config/temp.conf
            state: commit
            message: "Configuration update"
            package: /backup/backup.apk
            media: usb
            verbose: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/sbin/lbu".to_owned()),
                extra_args: Some("-p /custom/path".to_owned()),
                path: Some("/overlay".to_owned()),
                include: vec!["/etc/config/app.conf".to_owned()],
                exclude: vec!["/etc/config/temp.conf".to_owned()],
                state: Some(State::Commit),
                message: Some("Configuration update".to_owned()),
                package: Some("/backup/backup.apk".to_owned()),
                media: Some("usb".to_owned()),
                verbose: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: commit
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_lbu_client_new_with_nonexistent_executable() {
        let result = LbuClient::new(
            Path::new("definitely-not-a-real-executable"),
            None,
            None,
            false,
            false,
        );
        assert!(result.is_ok());
    }
}
