/// ANCHOR: module
/// # nginx
///
/// Manage Nginx web server site configurations.
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
/// - nginx:
///     name: mysite
///     state: present
///     config: |
///       server {
///          listen 80;
///          server_name example.com;
///          root /var/www/html;
///       }
///
/// - nginx:
///     name: oldsite
///     state: absent
///
/// - nginx:
///     name: mysite
///     state: present
///     template: /etc/rash/templates/mysite.conf.j2
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff_files;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Site name used as filename in sites directories.
    name: String,
    /// Whether the site should be present and enabled or absent and disabled.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: State,
    /// Path to the sites-available directory.
    /// **[default: `"/etc/nginx/sites-available"`]**
    #[serde(default = "default_sites_dir")]
    sites_dir: String,
    #[serde(flatten)]
    content: Option<Content>,
}

fn default_state() -> State {
    State::Present
}

fn default_sites_dir() -> String {
    "/etc/nginx/sites-available".to_owned()
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Content {
    /// Raw nginx configuration content.
    Config(String),
    /// Path to a configuration template file.
    Template(String),
}

fn get_enabled_dir(sites_dir: &str) -> String {
    let path = Path::new(sites_dir);
    let parent = path.parent().unwrap_or(Path::new("/etc/nginx"));
    parent
        .join("sites-enabled")
        .to_str()
        .unwrap_or("/etc/nginx/sites-enabled")
        .to_owned()
}

fn get_available_path(name: &str, sites_dir: &str) -> String {
    Path::new(sites_dir)
        .join(name)
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

fn get_enabled_path(name: &str, sites_dir: &str) -> String {
    Path::new(&get_enabled_dir(sites_dir))
        .join(name)
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

fn ensure_symlink(available_path: &str, enabled_path: &str, check_mode: bool) -> Result<bool> {
    let meta = fs::symlink_metadata(enabled_path);

    match meta {
        Ok(m) => {
            if m.file_type().is_symlink() {
                let target = fs::read_link(enabled_path)?;
                if target == Path::new(available_path) {
                    return Ok(false);
                }
                diff_files(
                    format!("symlink -> {}", target.display()),
                    format!("symlink -> {}", available_path),
                );
                if !check_mode {
                    fs::remove_file(enabled_path)?;
                    unix_fs::symlink(available_path, enabled_path)?;
                }
            } else {
                diff_files("file", format!("symlink -> {}", available_path));
                if !check_mode {
                    fs::remove_file(enabled_path)?;
                    let parent = Path::new(enabled_path).parent().ok_or_else(|| {
                        Error::new(ErrorKind::InvalidData, "Invalid sites-enabled path")
                    })?;
                    fs::create_dir_all(parent)?;
                    unix_fs::symlink(available_path, enabled_path)?;
                }
            }
            Ok(true)
        }
        Err(_) => {
            diff_files("(absent)", format!("symlink -> {}", available_path));
            if !check_mode {
                let parent = Path::new(enabled_path).parent().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Invalid sites-enabled path")
                })?;
                fs::create_dir_all(parent)?;
                unix_fs::symlink(available_path, enabled_path)?;
            }
            Ok(true)
        }
    }
}

fn write_config(params: &Params, check_mode: bool) -> Result<bool> {
    let content = match &params.content {
        Some(c) => c,
        None => return Ok(false),
    };

    let available_path = get_available_path(&params.name, &params.sites_dir);

    let desired_content = match content {
        Content::Config(s) => s.clone(),
        Content::Template(path) => fs::read_to_string(path)?,
    };

    let current_content = fs::read_to_string(&available_path).ok();

    if current_content.as_deref() == Some(&desired_content) {
        return Ok(false);
    }

    diff_files(
        current_content.as_deref().unwrap_or("(absent)"),
        &desired_content,
    );

    if !check_mode {
        let parent = Path::new(&available_path)
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid sites_dir path"))?;
        fs::create_dir_all(parent)?;
        fs::write(&available_path, &desired_content)?;
    }

    Ok(true)
}

fn exec_nginx(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let available_path = get_available_path(&params.name, &params.sites_dir);
    let enabled_path = get_enabled_path(&params.name, &params.sites_dir);
    let mut changed = false;

    match params.state {
        State::Present => {
            changed |= write_config(&params, check_mode)?;
            changed |= ensure_symlink(&available_path, &enabled_path, check_mode)?;
        }
        State::Absent => {
            if let Ok(m) = fs::symlink_metadata(&enabled_path) {
                let label = if m.file_type().is_symlink() {
                    "symlink"
                } else {
                    "file"
                };
                diff_files(label, "(absent)");
                if !check_mode {
                    fs::remove_file(&enabled_path)?;
                }
                changed = true;
            }

            if fs::metadata(&available_path).is_ok() {
                diff_files("file", "(absent)");
                if !check_mode {
                    fs::remove_file(&available_path)?;
                }
                changed = true;
            }
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(params.name.clone()),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Nginx;

impl Module for Nginx {
    fn get_name(&self) -> &str {
        "nginx"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            exec_nginx(parse_params(optional_params)?, check_mode)?,
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

    use tempfile::tempdir;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mysite
            state: present
            config: |
              server {
                  listen 80;
              }
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "mysite");
        assert_eq!(params.state, State::Present);
        assert_eq!(
            params.content,
            Some(Content::Config("server {\n    listen 80;\n}\n".to_owned()))
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: oldsite
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "oldsite");
        assert_eq!(params.state, State::Absent);
        assert!(params.content.is_none());
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mysite
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.sites_dir, "/etc/nginx/sites-available");
        assert!(params.content.is_none());
    }

    #[test]
    fn test_parse_params_custom_sites_dir() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mysite
            sites_dir: /opt/nginx/sites
            config: "test"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.sites_dir, "/opt/nginx/sites");
    }

    #[test]
    fn test_parse_params_template() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mysite
            template: /etc/rash/templates/site.conf.j2
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.content,
            Some(Content::Template(
                "/etc/rash/templates/site.conf.j2".to_owned()
            ))
        );
    }

    #[test]
    fn test_parse_params_no_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mysite
            invalid: true
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_exec_nginx_present_creates_config_and_symlink() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let config_content = "server {\n    listen 80;\n}\n";
        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Present,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: Some(Content::Config(config_content.to_owned())),
            },
            false,
        )
        .unwrap();

        assert!(result.changed);
        assert_eq!(result.output, Some("mysite".to_owned()));

        let available_file = sites_available.join("mysite");
        let enabled_link = sites_enabled.join("mysite");
        assert!(fs::metadata(&available_file).is_ok());
        assert_eq!(fs::read_to_string(&available_file).unwrap(), config_content);

        let meta = fs::symlink_metadata(&enabled_link).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(fs::read_link(&enabled_link).unwrap(), available_file);
    }

    #[test]
    fn test_exec_nginx_present_idempotent() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let config_content = "server {\n    listen 80;\n}\n";

        let params = Params {
            name: "mysite".to_owned(),
            state: State::Present,
            sites_dir: sites_available.to_str().unwrap().to_owned(),
            content: Some(Content::Config(config_content.to_owned())),
        };

        let result1 = exec_nginx(params.clone(), false).unwrap();
        assert!(result1.changed);

        let result2 = exec_nginx(params, false).unwrap();
        assert!(!result2.changed);
    }

    #[test]
    fn test_exec_nginx_present_check_mode() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Present,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: Some(Content::Config("server { listen 80; }".to_owned())),
            },
            true,
        )
        .unwrap();

        assert!(result.changed);
        assert!(fs::metadata(sites_available.join("mysite")).is_err());
        assert!(fs::symlink_metadata(sites_enabled.join("mysite")).is_err());
    }

    #[test]
    fn test_exec_nginx_absent_removes_symlink_and_config() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let available_file = sites_available.join("mysite");
        let enabled_link = sites_enabled.join("mysite");
        fs::write(&available_file, "server { listen 80; }").unwrap();
        unix_fs::symlink(&available_file, &enabled_link).unwrap();

        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Absent,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: None,
            },
            false,
        )
        .unwrap();

        assert!(result.changed);
        assert!(fs::metadata(&available_file).is_err());
        assert!(fs::symlink_metadata(&enabled_link).is_err());
    }

    #[test]
    fn test_exec_nginx_absent_idempotent() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let result = exec_nginx(
            Params {
                name: "nonexistent".to_owned(),
                state: State::Absent,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: None,
            },
            false,
        )
        .unwrap();

        assert!(!result.changed);
    }

    #[test]
    fn test_exec_nginx_absent_check_mode() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let available_file = sites_available.join("mysite");
        let enabled_link = sites_enabled.join("mysite");
        fs::write(&available_file, "server { listen 80; }").unwrap();
        unix_fs::symlink(&available_file, &enabled_link).unwrap();

        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Absent,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: None,
            },
            true,
        )
        .unwrap();

        assert!(result.changed);
        assert!(fs::metadata(&available_file).is_ok());
        assert!(fs::symlink_metadata(&enabled_link).is_ok());
    }

    #[test]
    fn test_exec_nginx_present_enable_existing_site() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let available_file = sites_available.join("mysite");
        fs::write(&available_file, "server { listen 80; }").unwrap();

        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Present,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: None,
            },
            false,
        )
        .unwrap();

        assert!(result.changed);
        let enabled_link = sites_enabled.join("mysite");
        let meta = fs::symlink_metadata(&enabled_link).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(
            fs::read_to_string(&available_file).unwrap(),
            "server { listen 80; }"
        );
    }

    #[test]
    fn test_exec_nginx_present_updates_config() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let available_file = sites_available.join("mysite");
        let enabled_link = sites_enabled.join("mysite");
        fs::write(&available_file, "server { listen 80; }").unwrap();
        unix_fs::symlink(&available_file, &enabled_link).unwrap();

        let new_config = "server {\n    listen 443 ssl;\n}\n";
        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Present,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: Some(Content::Config(new_config.to_owned())),
            },
            false,
        )
        .unwrap();

        assert!(result.changed);
        assert_eq!(fs::read_to_string(&available_file).unwrap(), new_config);
        assert!(
            fs::symlink_metadata(&enabled_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn test_exec_nginx_corrects_wrong_symlink() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("sites-available");
        let sites_enabled = dir.path().join("sites-enabled");
        fs::create_dir_all(&sites_available).unwrap();
        fs::create_dir_all(&sites_enabled).unwrap();

        let available_file = sites_available.join("mysite");
        let wrong_target = sites_available.join("other");
        let enabled_link = sites_enabled.join("mysite");
        fs::write(&available_file, "server { listen 80; }").unwrap();
        fs::write(&wrong_target, "other config").unwrap();
        unix_fs::symlink(&wrong_target, &enabled_link).unwrap();

        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Present,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: None,
            },
            false,
        )
        .unwrap();

        assert!(result.changed);
        assert_eq!(fs::read_link(&enabled_link).unwrap(), available_file);
    }

    #[test]
    fn test_exec_nginx_present_creates_dirs() {
        let dir = tempdir().unwrap();
        let sites_available = dir.path().join("nginx").join("sites-available");
        let sites_enabled = dir.path().join("nginx").join("sites-enabled");

        let result = exec_nginx(
            Params {
                name: "mysite".to_owned(),
                state: State::Present,
                sites_dir: sites_available.to_str().unwrap().to_owned(),
                content: Some(Content::Config("server { listen 80; }".to_owned())),
            },
            false,
        )
        .unwrap();

        assert!(result.changed);
        assert!(fs::metadata(&sites_available).is_ok());
        assert!(fs::metadata(sites_available.join("mysite")).is_ok());
        assert!(fs::symlink_metadata(sites_enabled.join("mysite")).is_ok());
    }
}
