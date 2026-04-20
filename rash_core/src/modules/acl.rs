/// ANCHOR: module
/// # acl
///
/// Manage file Access Control Lists (ACLs).
///
/// ACLs provide fine-grained permission control beyond standard Unix permissions.
/// They allow per-user and per-group permissions on files and directories.
/// Useful for containers, IoT devices, and multi-user file sharing scenarios.
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
/// - name: Give user nginx read access to a file
///   acl:
///     path: /etc/app/config.json
///     user: nginx
///     mode: "r"
///
/// - name: Give group developers read-write access
///   acl:
///     path: /data/project
///     group: developers
///     mode: "rw"
///
/// - name: Set default ACL for directory (inherited by new files)
///   acl:
///     path: /data/shared
///     user: appuser
///     mode: "rwx"
///     default: true
///
/// - name: Remove user ACL entry
///   acl:
///     path: /data/file.txt
///     user: olduser
///     state: absent
///
/// - name: Query current ACLs
///   acl:
///     path: /etc/app/config.json
///     state: query
///   register: file_acls
///
/// - name: Apply ACLs recursively
///   acl:
///     path: /data/project
///     user: nginx
///     mode: "rX"
///     recurse: true
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
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
    Query,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The full path to the file or directory.
    path: String,
    /// The user to set ACL for (e.g. "nginx").
    user: Option<String>,
    /// The group to set ACL for (e.g. "developers").
    group: Option<String>,
    /// The permissions mode (e.g. "r", "rw", "rwx", "rX").
    /// Required when state=present.
    mode: Option<String>,
    /// Whether the ACL should exist or not.
    /// Use query to retrieve current ACLs without changes.
    /// **[default: `"present"`]**
    #[serde(default)]
    state: Option<State>,
    /// Set default ACL (inherited by new files in directory).
    /// **[default: `false`]**
    #[serde(default)]
    default: bool,
    /// Apply ACLs recursively to directory contents.
    /// **[default: `false`]**
    #[serde(default)]
    recurse: bool,
}

fn run_command(cmd: &mut Command) -> Result<String> {
    let output = cmd
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "command '{}' failed with exit code {:?}: {}",
                format!("{:?}", cmd)
                    .trim_start_matches("Command { ")
                    .trim_end_matches(" }"),
                output.status.code(),
                stderr.trim()
            ),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn check_path_exists(path: &str) -> Result<()> {
    if !Path::new(path).exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("path '{}' does not exist", path),
        ));
    }
    Ok(())
}

fn build_acl_spec(params: &Params, prefix: &str) -> Result<String> {
    match (&params.user, &params.group) {
        (Some(user), _) => Ok(format!("{}u:{}:", prefix, user)),
        (_, Some(group)) => Ok(format!("{}g:{}:", prefix, group)),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            "either 'user' or 'group' must be specified",
        )),
    }
}

fn build_setfacl_args(params: &Params) -> Result<Vec<String>> {
    let mut args = Vec::new();

    let prefix = if params.default { "d:" } else { "" };

    if params.recurse {
        args.push("-R".to_string());
    }

    let acl_spec = build_acl_spec(params, prefix)?;
    let mode = params
        .mode
        .as_deref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "mode is required for state=present"))?;

    args.push("-m".to_string());
    args.push(format!("{}{}", acl_spec, mode));
    args.push("--".to_string());
    args.push(params.path.clone());

    Ok(args)
}

fn build_remove_acl_args(params: &Params) -> Result<Vec<String>> {
    let mut args = Vec::new();

    let prefix = if params.default { "d:" } else { "" };

    if params.recurse {
        args.push("-R".to_string());
    }

    let acl_spec = build_acl_spec(params, prefix)?;

    args.push("-x".to_string());
    args.push(acl_spec);
    args.push("--".to_string());
    args.push(params.path.clone());

    Ok(args)
}

fn parse_acl_entry(entry_type: &str, rest: &str, is_default: bool) -> Option<serde_json::Value> {
    let (qualifier, permissions) = if let Some((q, p)) = rest.rsplit_once(':') {
        (Some(q.to_string()), p)
    } else {
        (None, rest)
    };
    Some(json!({
        "type": entry_type,
        "qualifier": qualifier,
        "permissions": permissions,
        "default": is_default,
    }))
}

fn parse_getfacl_output(output: &str) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (line_content, is_default) = match line.strip_prefix("default:") {
            Some(rest) => (rest, true),
            None => (line, false),
        };

        if let Some(rest) = line_content.strip_prefix("user::") {
            entries.push(json!({
                "type": "user",
                "qualifier": null,
                "permissions": rest,
                "default": is_default,
            }));
        } else if let Some(rest) = line_content.strip_prefix("user:") {
            if let Some(entry) = parse_acl_entry("user", rest, is_default) {
                entries.push(entry);
            }
        } else if let Some(rest) = line_content.strip_prefix("group::") {
            entries.push(json!({
                "type": "group",
                "qualifier": null,
                "permissions": rest,
                "default": is_default,
            }));
        } else if let Some(rest) = line_content.strip_prefix("group:") {
            if let Some(entry) = parse_acl_entry("group", rest, is_default) {
                entries.push(entry);
            }
        } else if let Some(rest) = line_content.strip_prefix("other::") {
            entries.push(json!({
                "type": "other",
                "qualifier": null,
                "permissions": rest,
                "default": is_default,
            }));
        } else if let Some(rest) = line_content.strip_prefix("mask::") {
            entries.push(json!({
                "type": "mask",
                "qualifier": null,
                "permissions": rest,
                "default": is_default,
            }));
        }
    }

    entries
}

fn get_qualifier(params: &Params) -> Result<(&str, &str)> {
    let qualifier_type = if params.user.is_some() {
        "user"
    } else {
        "group"
    };
    let qualifier = params
        .user
        .as_deref()
        .or(params.group.as_deref())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "either 'user' or 'group' must be specified",
            )
        })?;
    Ok((qualifier_type, qualifier))
}

fn get_current_acl_for(
    path: &str,
    qualifier_type: &str,
    qualifier: &str,
    is_default: bool,
) -> Result<Option<String>> {
    let output = run_command(Command::new("getfacl").args([
        "--absolute-names",
        "--no-effective",
        "-p",
        "--",
        path,
    ]))?;

    let entries = parse_getfacl_output(&output);

    for entry in &entries {
        let entry_type = entry["type"].as_str().unwrap_or("");
        let entry_qualifier = entry["qualifier"].as_str().unwrap_or("");
        let entry_default = entry["default"].as_bool().unwrap_or(false);
        let perms = entry["permissions"].as_str().unwrap_or("");

        if entry_type == qualifier_type
            && entry_qualifier == qualifier
            && entry_default == is_default
        {
            return Ok(Some(perms.to_string()));
        }
    }

    Ok(None)
}

fn handle_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    check_path_exists(&params.path)?;
    let (qualifier_type, qualifier) = get_qualifier(params)?;

    let mode = params
        .mode
        .as_deref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "mode is required for state=present"))?;

    let current = get_current_acl_for(&params.path, qualifier_type, qualifier, params.default)?;

    if let Some(ref current_mode) = current {
        if current_mode == mode {
            return Ok(ModuleResult {
                changed: false,
                output: Some(params.path.clone()),
                extra: None,
            });
        }
        debug!(
            "ACL {}:{} changing from '{}' to '{}' on '{}'",
            qualifier_type, qualifier, current_mode, mode, params.path
        );
    } else {
        debug!(
            "Setting ACL {}:{} to '{}' on '{}'",
            qualifier_type, qualifier, mode, params.path
        );
    }

    if !check_mode {
        let args = build_setfacl_args(params)?;
        run_command(Command::new("setfacl").args(&args))?;
    }
    Ok(ModuleResult {
        changed: true,
        output: Some(params.path.clone()),
        extra: None,
    })
}

fn handle_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    check_path_exists(&params.path)?;
    let (qualifier_type, qualifier) = get_qualifier(params)?;

    let current = get_current_acl_for(&params.path, qualifier_type, qualifier, params.default)?;

    match current {
        Some(_) => {
            debug!(
                "Removing ACL {}:{} from '{}'",
                qualifier_type, qualifier, params.path
            );
            if !check_mode {
                let args = build_remove_acl_args(params)?;
                run_command(Command::new("setfacl").args(&args))?;
            }
            Ok(ModuleResult {
                changed: true,
                output: Some(params.path.clone()),
                extra: None,
            })
        }
        None => Ok(ModuleResult {
            changed: false,
            output: Some(params.path.clone()),
            extra: None,
        }),
    }
}

fn handle_query(params: &Params) -> Result<ModuleResult> {
    check_path_exists(&params.path)?;

    let output = run_command(Command::new("getfacl").args([
        "--absolute-names",
        "--no-effective",
        "-p",
        "--",
        &params.path,
    ]))?;

    let entries = parse_getfacl_output(&output);

    let extra = serde_norway::to_value(json!({
        "acls": entries
    }))
    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    Ok(ModuleResult {
        changed: false,
        output: Some(params.path.clone()),
        extra: Some(extra),
    })
}

pub fn acl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let state = params.state.as_ref().unwrap_or(&State::Present);

    match state {
        State::Present => handle_present(&params, check_mode),
        State::Absent => handle_absent(&params, check_mode),
        State::Query => handle_query(&params),
    }
}

#[derive(Debug)]
pub struct Acl;

impl Module for Acl {
    fn get_name(&self) -> &str {
        "acl"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((acl(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_present_user() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            user: nginx
            mode: "r"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/tmp/test");
        assert_eq!(params.user, Some("nginx".to_string()));
        assert_eq!(params.mode, Some("r".to_string()));
        assert_eq!(params.state, None);
        assert!(!params.default);
        assert!(!params.recurse);
    }

    #[test]
    fn test_parse_params_present_group() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            group: developers
            mode: "rw"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.group, Some("developers".to_string()));
        assert_eq!(params.mode, Some("rw".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            user: olduser
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_query() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            state: query
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Query));
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            user: appuser
            mode: "rwx"
            default: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.default);
    }

    #[test]
    fn test_parse_params_recurse() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            user: nginx
            mode: "rX"
            recurse: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.recurse);
    }

    #[test]
    fn test_parse_params_no_path() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: nginx
            mode: "r"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_build_acl_spec_user() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: Some("nginx".to_string()),
            group: None,
            mode: Some("r".to_string()),
            state: None,
            default: false,
            recurse: false,
        };
        assert_eq!(build_acl_spec(&params, "").unwrap(), "u:nginx:");
    }

    #[test]
    fn test_build_acl_spec_group() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: None,
            group: Some("devs".to_string()),
            mode: Some("rw".to_string()),
            state: None,
            default: false,
            recurse: false,
        };
        assert_eq!(build_acl_spec(&params, "").unwrap(), "g:devs:");
    }

    #[test]
    fn test_build_acl_spec_default() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: Some("nginx".to_string()),
            group: None,
            mode: Some("r".to_string()),
            state: None,
            default: true,
            recurse: false,
        };
        assert_eq!(build_acl_spec(&params, "d:").unwrap(), "d:u:nginx:");
    }

    #[test]
    fn test_build_acl_spec_no_user_or_group() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: None,
            group: None,
            mode: Some("r".to_string()),
            state: None,
            default: false,
            recurse: false,
        };
        assert!(build_acl_spec(&params, "").is_err());
    }

    #[test]
    fn test_build_setfacl_args() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: Some("nginx".to_string()),
            group: None,
            mode: Some("r".to_string()),
            state: None,
            default: false,
            recurse: false,
        };
        let args = build_setfacl_args(&params).unwrap();
        assert_eq!(args, vec!["-m", "u:nginx:r", "--", "/tmp/test"]);
    }

    #[test]
    fn test_build_setfacl_args_default_recurse() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: Some("nginx".to_string()),
            group: None,
            mode: Some("rwx".to_string()),
            state: None,
            default: true,
            recurse: true,
        };
        let args = build_setfacl_args(&params).unwrap();
        assert_eq!(args, vec!["-R", "-m", "d:u:nginx:rwx", "--", "/tmp/test"]);
    }

    #[test]
    fn test_build_remove_acl_args() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: Some("nginx".to_string()),
            group: None,
            mode: None,
            state: Some(State::Absent),
            default: false,
            recurse: false,
        };
        let args = build_remove_acl_args(&params).unwrap();
        assert_eq!(args, vec!["-x", "u:nginx:", "--", "/tmp/test"]);
    }

    #[test]
    fn test_build_remove_acl_args_default() {
        let params = Params {
            path: "/tmp/test".to_string(),
            user: Some("nginx".to_string()),
            group: None,
            mode: None,
            state: Some(State::Absent),
            default: true,
            recurse: false,
        };
        let args = build_remove_acl_args(&params).unwrap();
        assert_eq!(args, vec!["-x", "d:u:nginx:", "--", "/tmp/test"]);
    }

    #[test]
    fn test_parse_getfacl_output() {
        let output = "# file: /tmp/test\n# owner: root\n# group: root\nuser::rw-\nuser:nginx:r--\ngroup::r--\ngroup:devs:rw-\nmask::rw-\nother::r--\n";
        let entries = parse_getfacl_output(output);

        assert_eq!(entries.len(), 6);

        assert_eq!(entries[0]["type"], "user");
        assert_eq!(entries[0]["qualifier"], serde_json::Value::Null);
        assert_eq!(entries[0]["permissions"], "rw-");
        assert_eq!(entries[0]["default"], false);

        assert_eq!(entries[1]["type"], "user");
        assert_eq!(entries[1]["qualifier"], "nginx");
        assert_eq!(entries[1]["permissions"], "r--");

        assert_eq!(entries[2]["type"], "group");
        assert_eq!(entries[2]["qualifier"], serde_json::Value::Null);
        assert_eq!(entries[2]["permissions"], "r--");

        assert_eq!(entries[3]["type"], "group");
        assert_eq!(entries[3]["qualifier"], "devs");
        assert_eq!(entries[3]["permissions"], "rw-");

        assert_eq!(entries[4]["type"], "mask");
        assert_eq!(entries[4]["permissions"], "rw-");

        assert_eq!(entries[5]["type"], "other");
        assert_eq!(entries[5]["permissions"], "r--");
    }

    #[test]
    fn test_parse_getfacl_output_with_defaults() {
        let output = "# file: /tmp/test\nuser::rw-\ndefault:user:nginx:rwx\ndefault:group::r-x\ndefault:mask::rwx\ndefault:other::r-x\n";
        let entries = parse_getfacl_output(output);

        assert_eq!(entries.len(), 5);

        assert_eq!(entries[0]["type"], "user");
        assert_eq!(entries[0]["default"], false);

        assert_eq!(entries[1]["type"], "user");
        assert_eq!(entries[1]["qualifier"], "nginx");
        assert_eq!(entries[1]["permissions"], "rwx");
        assert_eq!(entries[1]["default"], true);

        assert_eq!(entries[2]["type"], "group");
        assert_eq!(entries[2]["default"], true);

        assert_eq!(entries[3]["type"], "mask");
        assert_eq!(entries[3]["default"], true);

        assert_eq!(entries[4]["type"], "other");
        assert_eq!(entries[4]["default"], true);
    }

    #[test]
    fn test_acl_not_exists() {
        let result = acl(
            Params {
                path: "/nonexistent/path".to_string(),
                user: Some("nginx".to_string()),
                group: None,
                mode: Some("r".to_string()),
                state: Some(State::Present),
                default: false,
                recurse: false,
            },
            false,
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_acl_present_missing_user_and_group() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::File::create(&file_path).unwrap();

        let result = acl(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                user: None,
                group: None,
                mode: Some("r".to_string()),
                state: Some(State::Present),
                default: false,
                recurse: false,
            },
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_acl_present_missing_mode() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::File::create(&file_path).unwrap();

        let result = acl(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                user: Some("nginx".to_string()),
                group: None,
                mode: None,
                state: Some(State::Present),
                default: false,
                recurse: false,
            },
            false,
        );
        assert!(result.is_err());
    }

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;
        use std::fs;

        fn getfacl_available() -> bool {
            std::process::Command::new("which")
                .arg("getfacl")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }

        fn setfacl_available() -> bool {
            std::process::Command::new("which")
                .arg("setfacl")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }

        fn acl_tools_available() -> bool {
            getfacl_available() && setfacl_available()
        }

        fn acl_supported_on_tmpdir() -> bool {
            if !acl_tools_available() {
                return false;
            }
            let dir = tempfile::tempdir().unwrap();
            let file_path = dir.path().join("acl_probe.txt");
            fs::File::create(&file_path).unwrap();
            let output = std::process::Command::new("setfacl")
                .args(["-m", "u:root:r", "--", file_path.to_str().unwrap()])
                .output();
            match output {
                Ok(o) => o.status.success(),
                Err(_) => false,
            }
        }

        #[test]
        fn test_acl_set_user_acl() {
            if !acl_supported_on_tmpdir() {
                eprintln!("skipping: ACL not supported on this filesystem");
                return;
            }

            let dir = tempfile::tempdir().unwrap();
            let file_path = dir.path().join("test.txt");
            fs::File::create(&file_path).unwrap();

            let result = acl(
                Params {
                    path: file_path.to_str().unwrap().to_string(),
                    user: Some("root".to_string()),
                    group: None,
                    mode: Some("r".to_string()),
                    state: Some(State::Present),
                    default: false,
                    recurse: false,
                },
                false,
            )
            .unwrap();

            assert!(result.changed);
        }

        #[test]
        fn test_acl_query() {
            if !acl_tools_available() {
                return;
            }

            let dir = tempfile::tempdir().unwrap();
            let file_path = dir.path().join("test.txt");
            fs::File::create(&file_path).unwrap();

            let result = acl(
                Params {
                    path: file_path.to_str().unwrap().to_string(),
                    user: None,
                    group: None,
                    mode: None,
                    state: Some(State::Query),
                    default: false,
                    recurse: false,
                },
                false,
            )
            .unwrap();

            assert!(!result.changed);
            assert!(result.extra.is_some());
        }

        #[test]
        fn test_acl_remove_absent_entry() {
            if !acl_tools_available() {
                return;
            }

            let dir = tempfile::tempdir().unwrap();
            let file_path = dir.path().join("test.txt");
            fs::File::create(&file_path).unwrap();

            let result = acl(
                Params {
                    path: file_path.to_str().unwrap().to_string(),
                    user: Some("nonexistentuser12345".to_string()),
                    group: None,
                    mode: None,
                    state: Some(State::Absent),
                    default: false,
                    recurse: false,
                },
                false,
            )
            .unwrap();

            assert!(!result.changed);
        }

        #[test]
        fn test_acl_check_mode() {
            if !acl_supported_on_tmpdir() {
                eprintln!("skipping: ACL not supported on this filesystem");
                return;
            }

            let dir = tempfile::tempdir().unwrap();
            let file_path = dir.path().join("test.txt");
            fs::File::create(&file_path).unwrap();

            let result = acl(
                Params {
                    path: file_path.to_str().unwrap().to_string(),
                    user: Some("root".to_string()),
                    group: None,
                    mode: Some("rwx".to_string()),
                    state: Some(State::Present),
                    default: false,
                    recurse: false,
                },
                true,
            )
            .unwrap();

            assert!(result.changed);

            let query_result = acl(
                Params {
                    path: file_path.to_str().unwrap().to_string(),
                    user: None,
                    group: None,
                    mode: None,
                    state: Some(State::Query),
                    default: false,
                    recurse: false,
                },
                false,
            )
            .unwrap();

            let extra = query_result.extra.unwrap();
            let acls = extra.get("acls").unwrap();
            let has_named_user_acl = acls
                .as_sequence()
                .map(|seq| {
                    seq.iter()
                        .any(|e| e["type"] == "user" && e["qualifier"] == "root")
                })
                .unwrap_or(false);

            assert!(!has_named_user_acl);
        }
    }
}
