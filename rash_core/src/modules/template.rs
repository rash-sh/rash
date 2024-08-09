/// ANCHOR: module
/// # template
///
/// Render [MiniJinja template](https://docs.rs/minijinja/latest/minijinja/syntax/index.html).
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
/// - template:
///     src: "template.j2"
///     dest: /tmp/MY_PASSWORD_FILE.txt
///     mode: "0400"
/// ```
/// ANCHOR_END: examples
use crate::error::Result;
use crate::modules::copy::copy_file;
use crate::modules::copy::{Input, Params as CopyParams};
use crate::modules::{parse_params, Module, ModuleResult};
use crate::utils::jinja2::render_string;
use minijinja::Value;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{metadata, read_to_string};
use std::os::unix::fs::PermissionsExt;

#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde_yaml::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of Jinja formatted template.
    /// This can be a relative or an absolute path.
    src: String,
    /// Absolute path where the file should be rendered to.
    dest: String,
    /// Permissions of the destination file or directory.
    /// The mode may also be the special string `preserve`.
    /// `preserve` means that the file will be given the same permissions as the source file.
    mode: Option<String>,
}

fn render_content(params: Params, vars: Value) -> Result<CopyParams> {
    let mode = match params.mode.as_deref() {
        Some("preserve") => {
            let src_metadata = metadata(&params.src)?;
            let src_permissions = src_metadata.permissions();
            // & 0o7777 to remove lead 100: 100644 -> 644
            Some(format!("{:o}", src_permissions.mode() & 0o7777))
        }
        _ => params.mode,
    };

    Ok(CopyParams {
        input: Input::Content(render_string(&read_to_string(params.src)?, &vars)?),
        dest: params.dest.clone(),
        mode,
    })
}

#[derive(Debug)]
pub struct Template;

impl Module for Template {
    fn get_name(&self) -> &str {
        "template"
    }

    fn exec(
        &self,
        optional_params: YamlValue,
        vars: Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        Ok((
            copy_file(
                render_content(parse_params(optional_params)?, vars.clone())?,
                check_mode,
            )?,
            vars,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<RootSchema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::error::ErrorKind;

    use std::fs::{set_permissions, File};
    use std::io::Write;

    use minijinja::context;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            src: "/tmp/foo.j2"
            dest: "/tmp/buu.txt"
            mode: "0600"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/tmp/foo.j2".to_owned(),
                dest: "/tmp/buu.txt".to_owned(),
                mode: Some("0600".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_mode_int() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            src: "/tmp/foo.j2"
            dest: "/tmp/buu.txt"
            mode: 0600
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/tmp/foo.j2".to_owned(),
                dest: "/tmp/buu.txt".to_owned(),
                mode: Some("0600".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_no_mode() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            src: "/tmp/boo.j2"
            dest: "/tmp/buu.txt"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/tmp/boo.j2".to_owned(),
                dest: "/tmp/buu.txt".to_owned(),
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            src: "/tmp/boo.j2"
            yea: foo
            dest: "/tmp/buu.txt"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_render_content() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("template.j2");
        let mut file = File::create(file_path.clone()).unwrap();
        #[allow(clippy::write_literal)]
        writeln!(file, "{}", "{{ boo }}").unwrap();

        let vars = context! { boo => "test" };

        let copy_params = render_content(
            Params {
                src: file_path.to_str().unwrap().to_owned(),
                dest: "/tmp/buu.txt".to_owned(),
                mode: Some("0644".to_owned()),
            },
            vars,
        )
        .unwrap();

        assert_eq!(copy_params.get_content().unwrap(), "test\n");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o644)
        );
    }

    #[test]
    fn test_render_content_with_preserve() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("template.j2");
        let mut file = File::create(file_path.clone()).unwrap();
        #[allow(clippy::write_literal)]
        writeln!(file, "{}", "{{ boo }}").unwrap();

        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o604);
        set_permissions(&file_path, permissions).unwrap();

        let vars = Value::from_serialize(context! { boo => "test" });

        let copy_params = render_content(
            Params {
                src: file_path.to_str().unwrap().to_owned(),
                dest: "/tmp/buu.txt".to_owned(),
                mode: Some("preserve".to_owned()),
            },
            vars,
        )
        .unwrap();

        assert_eq!(copy_params.get_content().unwrap(), "test\n");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o604)
        );
    }
}
