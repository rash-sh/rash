/// ANCHOR: module
/// # template
///
/// Render [Tera template](https://tera.netlify.app/docs/#templates).
///
/// ## Parameters
///
/// ```yaml
/// src:
///   type: string
///   required: true
///   description: |
///     Path of Tera formatted template.
///     This can be a relative or an absolute path.
/// dest:
///   type: string
///   required: true
///   description: Absolute path where the file should be rendered to.
/// mode:
///   type: string
///   description: Permissions of the destination file or directory.
/// ```
/// ## Examples
///
/// ```yaml
/// - template:
///     src: "template.j2"
///     dest: /tmp/MY_PASSWORD_FILE.txt
///     mode: "0400"
/// ```
/// ANCHOR_END: module
use crate::error::{Error, ErrorKind, Result};
use crate::modules::copy::copy_file;
use crate::modules::copy::Params as CopyParams;
use crate::modules::{get_param, ModuleResult};
use crate::utils::parse_octal;
use crate::vars::Vars;

use std::path::Path;

use tera::Tera;
use yaml_rust::Yaml;

#[derive(Debug, PartialEq)]
struct Params {
    src: String,
    dest: String,
    mode: u32,
}

fn parse_params(yaml: Yaml) -> Result<Params> {
    trace!("parse params: {:?}", yaml);
    let mode_string = get_param(&yaml, "mode").or_else(|e| match e.kind() {
        ErrorKind::NotFound => Ok("0644".to_string()),
        _ => Err(e),
    })?;
    Ok(Params {
        src: get_param(&yaml, "src")?,
        dest: get_param(&yaml, "dest")?,
        mode: parse_octal(&mode_string)?,
    })
}

fn render_content(params: Params, vars: Vars) -> Result<CopyParams> {
    let mut tera = Tera::default();
    tera.add_template_file(Path::new(&params.src), None)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    Ok(CopyParams::new(
        tera.render(&params.src, &vars)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?,
        params.dest.clone(),
        params.mode,
    ))
}

pub fn exec(optional_params: Yaml, vars: Vars) -> Result<(ModuleResult, Vars)> {
    Ok((
        copy_file(render_content(
            parse_params(optional_params)?,
            vars.clone(),
        )?)?,
        vars,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::vars;

    use std::fs::File;
    use std::io::Write;

    use tempfile::tempdir;
    use yaml_rust::YamlLoader;

    #[test]
    fn test_parse_params() {
        let yaml = YamlLoader::load_from_str(
            r#"
        src: "/tmp/foo.j2"
        dest: "/tmp/buu.txt"
        mode: "0600"
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/tmp/foo.j2".to_string(),
                dest: "/tmp/buu.txt".to_string(),
                mode: 0o600,
            }
        );
    }

    #[test]
    fn test_parse_params_mode_int() {
        let yaml = YamlLoader::load_from_str(
            r#"
        content: "boo"
        dest: "/tmp/buu.txt"
        mode: 0600
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let error = parse_params(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_no_mode() {
        let yaml = YamlLoader::load_from_str(
            r#"
        src: "/tmp/boo.j2"
        dest: "/tmp/buu.txt"
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/tmp/boo.j2".to_string(),
                dest: "/tmp/buu.txt".to_string(),
                mode: 0o644,
            }
        );
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
                mode: 0o644,
            },
            vars,
        )
        .unwrap();

        assert_eq!(copy_params.get_content(), "test\n");
    }
}
