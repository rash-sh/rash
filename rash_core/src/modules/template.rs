/// ANCHOR: module
/// # template
///
/// Render [Tera template](https://tera.netlify.app/docs/#templates).
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
use crate::error::{Error, ErrorKind, Result};
use crate::modules::copy::copy_file;
use crate::modules::copy::{Input, Params as CopyParams};
use crate::modules::{parse_params, ModuleResult};
use crate::vars::Vars;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::metadata;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde_yaml::Value;
use tera::Tera;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of Tera formatted template.
    /// This can be a relative or an absolute path.
    src: String,
    /// Absolute path where the file should be rendered to.
    dest: String,
    /// Permissions of the destination file or directory.
    /// The mode may also be the special string `preserve`.
    /// `preserve` means that the file will be given the same permissions as the source file.
    mode: Option<String>,
}

fn render_content(params: Params, vars: Vars) -> Result<CopyParams> {
    let mut tera = Tera::default();
    tera.add_template_file(Path::new(&params.src), None)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
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
        input: Input::Content(
            tera.render(&params.src, &vars)
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?,
        ),
        dest: params.dest.clone(),
        mode,
    })
}

pub fn exec(optional_params: Value, vars: Vars, check_mode: bool) -> Result<(ModuleResult, Vars)> {
    Ok((
        copy_file(
            render_content(parse_params(optional_params)?, vars.clone())?,
            check_mode,
        )?,
        vars,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::vars;

    use std::fs::{set_permissions, File};
    use std::io::Write;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: Value = serde_yaml::from_str(
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
                src: "/tmp/foo.j2".to_string(),
                dest: "/tmp/buu.txt".to_string(),
                mode: Some("0600".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_params_mode_int() {
        let yaml: Value = serde_yaml::from_str(
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
                src: "/tmp/foo.j2".to_string(),
                dest: "/tmp/buu.txt".to_string(),
                mode: Some("0600".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_params_no_mode() {
        let yaml: Value = serde_yaml::from_str(
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
                src: "/tmp/boo.j2".to_string(),
                dest: "/tmp/buu.txt".to_string(),
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: Value = serde_yaml::from_str(
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

        let vars = vars::from_iter(vec![("boo", "test")].into_iter());

        let copy_params = render_content(
            Params {
                src: file_path.to_str().unwrap().to_owned(),
                dest: "/tmp/buu.txt".to_string(),
                mode: Some("0644".to_string()),
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

        let vars = vars::from_iter(vec![("boo", "test")].into_iter());

        let copy_params = render_content(
            Params {
                src: file_path.to_str().unwrap().to_owned(),
                dest: "/tmp/buu.txt".to_string(),
                mode: Some("preserve".to_string()),
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
