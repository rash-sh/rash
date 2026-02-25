/// ANCHOR: module
/// # chroot
///
/// Execute commands within a chroot environment.
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
/// - name: Update package lists in chroot
///   chroot:
///     root: /mnt
///     cmd: apt-get update
///
/// - name: Install packages in chroot
///   chroot:
///     root: /mnt
///     cmd: apt-get install -y linux-image-generic zfs-initramfs
///     environment:
///       DEBIAN_FRONTEND: noninteractive
///
/// - name: Run script in chroot
///   chroot:
///     root: /mnt
///     cmd: /usr/local/bin/setup-script.sh
///     creates: /etc/installed-marker
///
/// - name: Run as different user
///   chroot:
///     root: /mnt
///     cmd: whoami
///     become: true
///     become_user: agil
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use tempfile::TempDir;

const DEFAULT_TIMEOUT: u64 = 3600;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum CmdSpec {
    /// The command to run as a string.
    Cmd(String),
    /// The command to run as a list of arguments.
    Argv(Vec<String>),
    /// The command executable (use with args).
    Command(String),
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the chroot directory.
    pub root: String,
    #[serde(flatten)]
    pub cmd_spec: CmdSpec,
    /// Arguments for the command (if command used instead of cmd).
    pub args: Option<String>,
    /// Working directory inside chroot.
    /// **[default: `"/"`]**
    pub chdir: Option<String>,
    /// Data to pass to command stdin.
    pub stdin: Option<String>,
    /// Shell to use for command execution.
    /// **[default: `"/bin/sh"`]**
    pub executable: Option<String>,
    /// File that if exists skips the command.
    pub creates: Option<String>,
    /// File that if missing skips the command.
    pub removes: Option<String>,
    /// Environment variables to set.
    pub environment: Option<HashMap<String, String>>,
    /// File to source environment from (e.g., /etc/environment).
    pub env_file: Option<String>,
    /// Umask for command execution.
    pub umask: Option<u32>,
    /// Become another user inside chroot.
    #[serde(rename = "become")]
    pub do_become: Option<bool>,
    /// User to become.
    pub become_user: Option<String>,
    /// Command timeout in seconds.
    /// **[default: `3600`]**
    pub timeout: Option<u64>,
}

fn resolve_path_in_chroot(root: &str, path: &str) -> String {
    if path.starts_with('/') {
        format!("{}{}", root, path)
    } else {
        format!("{}/{}", root, path)
    }
}

fn check_creates(root: &str, creates: &str) -> bool {
    let full_path = resolve_path_in_chroot(root, creates);
    Path::new(&full_path).exists()
}

fn check_removes(root: &str, removes: &str) -> bool {
    let full_path = resolve_path_in_chroot(root, removes);
    !Path::new(&full_path).exists()
}

fn build_command(params: &Params) -> (String, Vec<String>) {
    let executable = params.executable.as_deref().unwrap_or("/bin/sh");

    match &params.cmd_spec {
        CmdSpec::Cmd(cmd) => (executable.to_string(), vec!["-c".to_string(), cmd.clone()]),
        CmdSpec::Argv(argv) => {
            if argv.is_empty() {
                (executable.to_string(), vec![])
            } else {
                (argv[0].clone(), argv[1..].to_vec())
            }
        }
        CmdSpec::Command(command) => {
            let mut cmd_args: Vec<String> = vec![command.clone()];
            if let Some(extra_args) = &params.args {
                cmd_args.extend(extra_args.split_whitespace().map(|s| s.to_string()));
            }
            (
                executable.to_string(),
                vec!["-c".to_string(), cmd_args.join(" ")],
            )
        }
    }
}

fn parse_env_file(path: &str) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read env file '{}': {}", path, e),
        )
    })?;

    let mut env_vars = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            env_vars.insert(key, value);
        }
    }

    Ok(env_vars)
}

fn merge_environment(params: &Params, root: &str) -> Result<HashMap<String, String>> {
    let mut env_vars: HashMap<String, String> = HashMap::new();

    env_vars.insert(
        "PATH".to_string(),
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
    );
    env_vars.insert("HOME".to_string(), "/root".to_string());
    env_vars.insert("TERM".to_string(), "dumb".to_string());

    if let Some(env_file) = &params.env_file {
        let full_path = resolve_path_in_chroot(root, env_file);
        let file_env = parse_env_file(&full_path)?;
        env_vars.extend(file_env);
    }

    if let Some(environment) = &params.environment {
        env_vars.extend(environment.clone());
    }

    Ok(env_vars)
}

struct ChrootExecutor {
    root: PathBuf,
    chdir: PathBuf,
    do_become: bool,
    become_user: Option<String>,
    umask: Option<u32>,
    timeout: u64,
}

impl ChrootExecutor {
    fn new(params: &Params) -> Self {
        let workdir = params
            .chdir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));

        ChrootExecutor {
            root: PathBuf::from(&params.root),
            chdir: workdir,
            do_become: params.do_become.unwrap_or(false),
            become_user: params.become_user.clone(),
            umask: params.umask,
            timeout: params.timeout.unwrap_or(DEFAULT_TIMEOUT),
        }
    }

    #[allow(dead_code)]
    fn timeout(&self) -> u64 {
        self.timeout
    }

    fn create_wrapper_script(
        &self,
        program: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
    ) -> Result<TempDir> {
        let temp_dir = TempDir::new().map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to create temp dir: {}", e),
            )
        })?;

        let wrapper_path = temp_dir.path().join("chroot_wrapper.sh");
        let mut script = String::new();

        script.push_str("#!/bin/sh\n");
        script.push_str("set -e\n");

        for (key, value) in env_vars {
            script.push_str(&format!(
                "export {}='{}'\n",
                key,
                value.replace("'", "'\\''")
            ));
        }

        if let Some(mask) = self.umask {
            script.push_str(&format!("umask {:03o}\n", mask));
        }

        script.push_str("cd \"");
        script.push_str(&self.chdir.to_string_lossy());
        script.push_str("\"\n");

        if self.do_become {
            if let Some(user) = &self.become_user {
                script.push_str(&format!("exec su - '{}' -c ", user));
                let full_cmd = if args.is_empty() {
                    format!("'{}'", program)
                } else {
                    format!("'{} {}'", program, args.join(" "))
                };
                script.push_str(&format!("\"{}\"\n", full_cmd.replace("\"", "\\\"")));
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "become_user is required when become is true",
                ));
            }
        } else {
            script.push_str("exec '");
            script.push_str(program);
            script.push('\'');
            for arg in args {
                script.push_str(" '");
                script.push_str(&arg.replace("'", "'\\''"));
                script.push('\'');
            }
            script.push('\n');
        }

        let mut file = fs::File::create(&wrapper_path).map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to create wrapper script: {}", e),
            )
        })?;

        file.write_all(script.as_bytes()).map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to write wrapper script: {}", e),
            )
        })?;

        fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755)).map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to set permissions on wrapper script: {}", e),
            )
        })?;

        Ok(temp_dir)
    }

    fn execute(
        &self,
        program: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        stdin_data: Option<&str>,
    ) -> Result<(String, String, i32)> {
        let temp_dir = self.create_wrapper_script(program, args, env_vars)?;
        let wrapper_path = temp_dir.path().join("chroot_wrapper.sh");
        let wrapper_in_chroot = format!("/tmp/chroot_wrapper_{}.sh", std::process::id());

        let full_wrapper_dest = self.root.join(&wrapper_in_chroot);
        if let Some(parent) = full_wrapper_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Error::new(
                    ErrorKind::IOError,
                    format!("Failed to create parent dir: {}", e),
                )
            })?;
        }

        fs::copy(&wrapper_path, &full_wrapper_dest).map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to copy wrapper script: {}", e),
            )
        })?;

        let mut cmd = Command::new("chroot");
        cmd.arg(&self.root);
        cmd.arg(&wrapper_in_chroot);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        if let Some(data) = stdin_data {
            let mut child = cmd.stdin(Stdio::piped()).spawn().map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to spawn chroot: {}", e),
                )
            })?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(data.as_bytes()).map_err(|e| {
                    Error::new(ErrorKind::IOError, format!("Failed to write stdin: {}", e))
                })?;
            }

            let output = child.wait_with_output().map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to wait for chroot: {}", e),
                )
            })?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let rc = output.status.code().unwrap_or(-1);

            let _ = fs::remove_file(self.root.join(&wrapper_in_chroot));

            Ok((stdout, stderr, rc))
        } else {
            let output = cmd.output().map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute chroot: {}", e),
                )
            })?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let rc = output.status.code().unwrap_or(-1);

            let _ = fs::remove_file(self.root.join(&wrapper_in_chroot));

            Ok((stdout, stderr, rc))
        }
    }
}

fn chroot_module(params: Params, _check_mode: bool) -> Result<ModuleResult> {
    let root_path = Path::new(&params.root);
    if !root_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Chroot root '{}' does not exist", params.root),
        ));
    }

    if let Some(creates) = &params.creates
        && check_creates(&params.root, creates)
    {
        return Ok(ModuleResult::new(false, None, None));
    }

    if let Some(removes) = &params.removes
        && check_removes(&params.root, removes)
    {
        return Ok(ModuleResult::new(false, None, None));
    }

    let (program, args) = build_command(&params);
    let env_vars = merge_environment(&params, &params.root)?;

    let executor = ChrootExecutor::new(&params);
    let (stdout, stderr, rc) =
        executor.execute(&program, &args, &env_vars, params.stdin.as_deref())?;

    if rc != 0 {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Command failed with exit code {}: {}", rc, stderr),
        ));
    }

    let output = if stdout.trim().is_empty() {
        None
    } else {
        Some(stdout.trim().to_string())
    };

    let cmd_str = match &params.cmd_spec {
        CmdSpec::Cmd(cmd) => cmd.clone(),
        CmdSpec::Argv(argv) => argv.join(" "),
        CmdSpec::Command(cmd) => {
            if let Some(cmd_args) = &params.args {
                format!("{} {}", cmd, cmd_args)
            } else {
                cmd.clone()
            }
        }
    };

    let extra = Some(value::to_value(json!({
        "rc": rc,
        "stderr": stderr,
        "cmd": cmd_str,
    }))?);

    Ok(ModuleResult::new(true, extra, output))
}

#[derive(Debug)]
pub struct Chroot;

impl Module for Chroot {
    fn get_name(&self) -> &str {
        "chroot"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            chroot_module(parse_params(optional_params)?, _check_mode)?,
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

    #[test]
    fn test_parse_params_cmd() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            cmd: apt-get update
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.root, "/mnt");
        assert_eq!(params.cmd_spec, CmdSpec::Cmd("apt-get update".to_owned()));
        assert_eq!(params.chdir, None);
    }

    #[test]
    fn test_parse_params_argv() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            argv:
              - echo
              - "hello world"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.root, "/mnt");
        assert_eq!(
            params.cmd_spec,
            CmdSpec::Argv(vec!["echo".to_owned(), "hello world".to_owned()])
        );
    }

    #[test]
    fn test_parse_params_with_environment() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            cmd: apt-get install -y vim
            environment:
              DEBIAN_FRONTEND: noninteractive
              FOO: bar
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.environment,
            Some(HashMap::from([
                ("DEBIAN_FRONTEND".to_owned(), "noninteractive".to_owned()),
                ("FOO".to_owned(), "bar".to_owned())
            ]))
        );
    }

    #[test]
    fn test_parse_params_with_become() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            cmd: whoami
            become: true
            become_user: agil
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.do_become, Some(true));
        assert_eq!(params.become_user, Some("agil".to_owned()));
    }

    #[test]
    fn test_parse_params_with_creates() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            cmd: /setup.sh
            creates: /etc/setup-done
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.creates, Some("/etc/setup-done".to_owned()));
    }

    #[test]
    fn test_parse_params_with_removes() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            cmd: rm /tmp/file
            removes: /tmp/file
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.removes, Some("/tmp/file".to_owned()));
    }

    #[test]
    fn test_resolve_path_in_chroot() {
        assert_eq!(
            resolve_path_in_chroot("/mnt", "/etc/passwd"),
            "/mnt/etc/passwd"
        );
        assert_eq!(
            resolve_path_in_chroot("/mnt", "relative/path"),
            "/mnt/relative/path"
        );
    }

    #[test]
    fn test_build_command_cmd() {
        let params = Params {
            root: "/mnt".to_string(),
            cmd_spec: CmdSpec::Cmd("echo hello".to_string()),
            args: None,
            chdir: None,
            stdin: None,
            executable: None,
            creates: None,
            removes: None,
            environment: None,
            env_file: None,
            umask: None,
            do_become: None,
            become_user: None,
            timeout: None,
        };

        let (program, args) = build_command(&params);
        assert_eq!(program, "/bin/sh");
        assert_eq!(args, vec!["-c", "echo hello"]);
    }

    #[test]
    fn test_build_command_argv() {
        let params = Params {
            root: "/mnt".to_string(),
            cmd_spec: CmdSpec::Argv(vec!["/usr/bin/apt-get".to_string(), "update".to_string()]),
            args: None,
            chdir: None,
            stdin: None,
            executable: None,
            creates: None,
            removes: None,
            environment: None,
            env_file: None,
            umask: None,
            do_become: None,
            become_user: None,
            timeout: None,
        };

        let (program, args) = build_command(&params);
        assert_eq!(program, "/usr/bin/apt-get");
        assert_eq!(args, vec!["update"]);
    }

    #[test]
    fn test_parse_env_file() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join("env");
        let mut file = fs::File::create(&env_path).unwrap();
        writeln!(file, "FOO=bar").unwrap();
        writeln!(file, "# comment").unwrap();
        writeln!(file, "BAZ=\"qux quux\"").unwrap();
        writeln!(file, "EMPTY=").unwrap();

        let env_vars = parse_env_file(env_path.to_str().unwrap()).unwrap();
        assert_eq!(env_vars.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(env_vars.get("BAZ"), Some(&"qux quux".to_string()));
        assert_eq!(env_vars.get("EMPTY"), Some(&"".to_string()));
        assert!(!env_vars.contains_key("comment"));
    }

    #[test]
    fn test_merge_environment() {
        let params = Params {
            root: "/mnt".to_string(),
            cmd_spec: CmdSpec::Cmd("echo".to_string()),
            args: None,
            chdir: None,
            stdin: None,
            executable: None,
            creates: None,
            removes: None,
            environment: Some(HashMap::from([("CUSTOM".to_string(), "value".to_string())])),
            env_file: None,
            umask: None,
            do_become: None,
            become_user: None,
            timeout: None,
        };

        let env_vars = merge_environment(&params, "/mnt").unwrap();
        assert!(env_vars.contains_key("PATH"));
        assert!(env_vars.contains_key("HOME"));
        assert_eq!(env_vars.get("CUSTOM"), Some(&"value".to_string()));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            cmd: ls
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_root() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: ls
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_cmd() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            root: /mnt
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
