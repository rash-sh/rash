/// ANCHOR: module
/// # git
///
/// Manage git checkouts of repositories.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// diff_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Clone application
///   git:
///     repo: https://github.com/user/app.git
///     dest: /opt/app
///     version: v1.2.0
///
/// - name: Clone with SSH
///   git:
///     repo: git@github.com:user/private-config.git
///     dest: /etc/app/config
///     key_file: /root/.ssh/deploy_key
///     accept_hostkey: yes
///
/// - name: Shallow clone
///   git:
///     repo: https://github.com/user/large-repo.git
///     dest: /opt/repo
///     depth: 1
///     single_branch: yes
///     version: main
///
/// - name: Update existing clone
///   git:
///     repo: https://github.com/user/app.git
///     dest: /opt/app
///     update: yes
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
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The repository URL to clone.
    pub repo: String,
    /// The destination path where the repository should be cloned.
    pub dest: String,
    /// The version to checkout. Can be a branch, tag, or commit hash.
    #[serde(default = "default_version")]
    pub version: String,
    /// Create a shallow clone with a history truncated to the specified number of commits.
    pub depth: Option<u32>,
    /// Clone only the specified branch.
    #[serde(default)]
    pub single_branch: bool,
    /// Update an existing repository to the latest revision.
    #[serde(default = "default_update")]
    pub update: bool,
    /// Path to the SSH private key file to use for authentication.
    pub key_file: Option<String>,
    /// Automatically accept the host key when connecting via SSH.
    #[serde(default)]
    pub accept_hostkey: bool,
    /// Force a reset to the specified version, discarding any local changes.
    #[serde(default)]
    pub force: bool,
}

fn default_version() -> String {
    "HEAD".to_string()
}

fn default_update() -> bool {
    true
}

fn run_git_command(
    args: &[&str],
    cwd: Option<&Path>,
    env: Option<&[(&str, &str)]>,
) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    if let Some(env_vars) = env {
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute git command: {e}"),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Git command failed: {stderr}"),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

fn get_current_branch(path: &Path) -> Result<String> {
    run_git_command(&["rev-parse", "--abbrev-ref", "HEAD"], Some(path), None)
}

fn get_current_commit(path: &Path) -> Result<String> {
    run_git_command(&["rev-parse", "HEAD"], Some(path), None)
}

fn get_remote_url(path: &Path) -> Result<String> {
    run_git_command(&["remote", "get-url", "origin"], Some(path), None)
}

fn has_local_changes(path: &Path) -> Result<bool> {
    let output = run_git_command(&["status", "--porcelain"], Some(path), None)?;
    Ok(!output.is_empty())
}

fn build_ssh_cmd(key_file: &str, accept_hostkey: bool) -> String {
    if accept_hostkey {
        format!("ssh -i {} -o StrictHostKeyChecking=no", key_file)
    } else {
        format!("ssh -i {}", key_file)
    }
}

fn clone_repo(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let dest_path = Path::new(&params.dest);

    if dest_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Destination {} already exists", params.dest),
        ));
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would clone {} to {}", params.repo, params.dest)),
            extra: None,
        });
    }

    let mut args: Vec<String> = vec!["clone".to_string()];

    if params.single_branch {
        args.push("--single-branch".to_string());
    }

    if let Some(depth) = params.depth {
        args.push("--depth".to_string());
        args.push(depth.to_string());
    }

    if params.version != "HEAD" && params.version != "master" {
        args.push("--branch".to_string());
        args.push(params.version.clone());
    }

    args.push(params.repo.clone());
    args.push(params.dest.clone());

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    if let Some(key_file) = &params.key_file {
        let ssh_cmd = build_ssh_cmd(key_file, params.accept_hostkey);
        let env = [("GIT_SSH_COMMAND", ssh_cmd.as_str())];
        run_git_command(&args_refs, None, Some(&env))?;
    } else {
        run_git_command(&args_refs, None, None)?;
    }

    let extra = json!({
        "repo": params.repo,
        "dest": params.dest,
        "version": params.version,
        "changed": true,
    });
    let extra = Some(value::to_value(extra)?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Cloned {} to {}", params.repo, params.dest)),
        extra,
    })
}

fn do_update_repo(
    dest_path: &Path,
    version: &str,
    force: bool,
    env: Option<&[(&str, &str)]>,
) -> Result<()> {
    run_git_command(&["fetch", "origin"], Some(dest_path), env)?;

    if force {
        run_git_command(
            &["reset", "--hard", &format!("origin/{}", version)],
            Some(dest_path),
            None,
        )?;
    } else {
        if version != "HEAD" {
            run_git_command(&["checkout", version], Some(dest_path), None)?;
        }

        run_git_command(&["pull", "origin", version], Some(dest_path), env)?;
    }

    Ok(())
}

fn update_repo(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let dest_path = Path::new(&params.dest);

    if !is_git_repo(dest_path) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("{} is not a git repository", params.dest),
        ));
    }

    let current_remote = get_remote_url(dest_path)?;
    if current_remote != params.repo {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Repository URL mismatch: expected {}, found {}",
                params.repo, current_remote
            ),
        ));
    }

    let before_commit = get_current_commit(dest_path)?;

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would update {} at {}", params.repo, params.dest)),
            extra: None,
        });
    }

    if has_local_changes(dest_path)? && !params.force {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Local changes detected. Use force=yes to discard them.".to_string(),
        ));
    }

    if let Some(key_file) = &params.key_file {
        let ssh_cmd = build_ssh_cmd(key_file, params.accept_hostkey);
        let env = [("GIT_SSH_COMMAND", ssh_cmd.as_str())];
        do_update_repo(dest_path, &params.version, params.force, Some(&env))?;
    } else {
        do_update_repo(dest_path, &params.version, params.force, None)?;
    }

    let after_commit = get_current_commit(dest_path)?;
    let changed = before_commit != after_commit;

    let extra = json!({
        "repo": params.repo,
        "dest": params.dest,
        "version": params.version,
        "before": before_commit,
        "after": after_commit,
        "changed": changed,
    });
    let extra = Some(value::to_value(extra)?);

    let output = if changed {
        format!(
            "Updated {} from {} to {}",
            params.dest, before_commit, after_commit
        )
    } else {
        format!("{} is already up to date", params.dest)
    };

    Ok(ModuleResult {
        changed,
        output: Some(output),
        extra,
    })
}

fn manage_git(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let dest_path = Path::new(&params.dest);

    if dest_path.exists() && is_git_repo(dest_path) {
        if params.update {
            update_repo(&params, check_mode)
        } else {
            let current_commit = get_current_commit(dest_path)?;
            let current_branch = get_current_branch(dest_path)?;

            let extra = json!({
                "repo": params.repo,
                "dest": params.dest,
                "version": current_branch,
                "commit": current_commit,
                "changed": false,
            });
            let extra = Some(value::to_value(extra)?);

            Ok(ModuleResult {
                changed: false,
                output: Some(format!(
                    "Repository {} exists at {}",
                    params.dest, current_commit
                )),
                extra,
            })
        }
    } else if dest_path.exists() {
        Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Destination {} exists but is not a git repository",
                params.dest
            ),
        ))
    } else {
        clone_repo(&params, check_mode)
    }
}

#[derive(Debug)]
pub struct Git;

impl Module for Git {
    fn get_name(&self) -> &str {
        "git"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((manage_git(params, check_mode)?, None))
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
    fn test_parse_params_simple() {
        let yaml = r#"
repo: "https://github.com/user/app.git"
dest: "/opt/app"
"#;
        let value: YamlValue = serde_norway::from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.repo, "https://github.com/user/app.git");
        assert_eq!(params.dest, "/opt/app");
        assert_eq!(params.version, "HEAD");
        assert!(params.update);
        assert!(!params.single_branch);
        assert!(!params.force);
    }

    #[test]
    fn test_parse_params_with_version() {
        let yaml = r#"
repo: "https://github.com/user/app.git"
dest: "/opt/app"
version: "v1.2.0"
"#;
        let value: YamlValue = serde_norway::from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.version, "v1.2.0");
    }

    #[test]
    fn test_parse_params_with_options() {
        let yaml = r#"
repo: "https://github.com/user/app.git"
dest: "/opt/app"
version: "main"
depth: 1
single_branch: true
force: true
key_file: "/root/.ssh/deploy_key"
accept_hostkey: true
"#;
        let value: YamlValue = serde_norway::from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.version, "main");
        assert_eq!(params.depth, Some(1));
        assert!(params.single_branch);
        assert!(params.force);
        assert_eq!(params.key_file, Some("/root/.ssh/deploy_key".to_string()));
        assert!(params.accept_hostkey);
    }

    #[test]
    fn test_is_git_repo() {
        let dir = tempdir().unwrap();
        assert!(!is_git_repo(dir.path()));

        fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(is_git_repo(dir.path()));
    }

    #[test]
    fn test_clone_repo_check_mode() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("clone");

        let params = Params {
            repo: "https://github.com/user/app.git".to_string(),
            dest: dest.to_str().unwrap().to_string(),
            version: "main".to_string(),
            depth: None,
            single_branch: false,
            update: true,
            key_file: None,
            accept_hostkey: false,
            force: false,
        };

        let result = clone_repo(&params, true).unwrap();
        assert!(result.changed);
        assert!(result.output.unwrap().contains("Would clone"));
        assert!(!dest.exists());
    }
}
