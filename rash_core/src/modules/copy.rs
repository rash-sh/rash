/// ANCHOR: module
/// # copy
///
/// Copy files to path.
///
/// ## Parameters
///
/// ```yaml
/// content:
///   type: string
///   required: true
///   description: Sets the contents of a file directly to the specified value.
/// dest:
///   type: string
///   required: true
///   description: Absolute path where the file should be copied to.
/// mode:
///   type: string
///   description: Permissions of the destination file or directory.
/// ```
/// ANCHOR_END: module
use crate::error::{ErrorKind, Result};
use crate::modules::{get_param, ModuleResult};
use crate::utils::parse_octal;
use crate::vars::Vars;

use std::fs::{set_permissions, OpenOptions};
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::{BufReader, Write};
use std::os::unix::fs::PermissionsExt;

use yaml_rust::Yaml;

#[derive(Debug, PartialEq)]
pub struct Params {
    content: String,
    dest: String,
    mode: u32,
}

impl Params {
    pub fn new(content: String, dest: String, mode: u32) -> Self {
        Params {
            content,
            dest,
            mode,
        }
    }

    #[cfg(test)]
    pub fn get_content(&self) -> String {
        self.content.clone()
    }
}

fn parse_params(yaml: Yaml) -> Result<Params> {
    trace!("parse params: {:?}", yaml);
    let mode_string = get_param(&yaml, "mode").or_else(|e| match e.kind() {
        ErrorKind::NotFound => Ok("0644".to_string()),
        _ => Err(e),
    })?;
    Ok(Params {
        content: get_param(&yaml, "content")?,
        dest: get_param(&yaml, "dest")?,
        mode: parse_octal(&mode_string)?,
    })
}

pub fn verify_file(params: Params) -> Result<ModuleResult> {
    trace!("params: {:?}", params);
    let open_read_file = OpenOptions::new().read(true).clone();
    let read_file = open_read_file.clone().open(&params.dest).or_else(|_| {
        trace!("file does not exists, create new one: {:?}", &params.dest);
        open_read_file
            .clone()
            .write(true)
            .create(true)
            .open(&params.dest)
    })?;
    let mut buf_reader = BufReader::new(&read_file);
    let mut content = String::new();
    buf_reader.read_to_string(&mut content)?;
    let metadata = read_file.metadata()?;
    let mut permissions = metadata.permissions();
    let mut changed = false;

    if content != params.content {
        trace!("changing content: {:?}", &params.content);
        if permissions.readonly() {
            let mut p = permissions.clone();
            // enable write
            p.set_mode(permissions.mode() | 0o200);
            set_permissions(&params.dest, p)?;
        }

        let mut file = OpenOptions::new().write(true).open(&params.dest)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(params.content.as_bytes())?;
        file.set_len(params.content.len() as u64)?;

        if permissions.readonly() {
            set_permissions(&params.dest, permissions.clone())?;
        }
        changed = true;
    };

    // & 0o7777 to remove lead 100: 100644 -> 644
    if permissions.mode() & 0o7777 != params.mode {
        trace!("changing mode: {:o}", &params.mode);
        permissions.set_mode(params.mode);
        set_permissions(&params.dest, permissions)?;
        changed = true;
    };

    Ok(ModuleResult {
        changed,
        output: Some(params.dest),
        extra: None,
    })
}

pub fn exec(optional_params: Yaml, vars: Vars) -> Result<(ModuleResult, Vars)> {
    Ok((verify_file(parse_params(optional_params)?)?, vars))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::File;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;
    use yaml_rust::YamlLoader;

    #[test]
    fn test_parse_params() {
        let yaml = YamlLoader::load_from_str(
            r#"
        content: "boo"
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
                content: "boo".to_string(),
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
        content: "boo"
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
                content: "boo".to_string(),
                dest: "/tmp/buu.txt".to_string(),
                mode: 0o644,
            }
        );
    }

    #[test]
    fn test_verify_file_no_change() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("no_change.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "test").unwrap();

        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o644);

        let output = verify_file(Params {
            content: "test\n".to_string(),
            dest: file_path.to_str().unwrap().to_string(),
            mode: 0o644,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_verify_file_change() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("change.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "test").unwrap();
        let output = verify_file(Params {
            content: "fu".to_string(),
            dest: file_path.to_str().unwrap().to_string(),
            mode: 0o400,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );

        let mut file = File::open(file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "fu");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o777),
            format!("{:o}", 0o400)
        );
    }

    #[test]
    fn test_verify_file_create() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("create.txt");
        let output = verify_file(Params {
            content: "zoo".to_string(),
            dest: file_path.to_str().unwrap().to_string(),
            mode: 0o400,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );

        let mut file = File::open(file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "zoo");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o777),
            format!("{:o}", 0o400)
        );
    }

    #[test]
    fn test_verify_file_read_only() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("read_only.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "read_only").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = verify_file(Params {
            content: "zoo".to_string(),
            dest: file_path.to_str().unwrap().to_string(),
            mode: 0o600,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );

        let mut file = File::open(file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "zoo");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o777),
            format!("{:o}", 0o600)
        );
    }

    #[test]
    fn test_verify_file_read_only_no_change_permissions() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("read_only.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "read_only").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = verify_file(Params {
            content: "zoo".to_string(),
            dest: file_path.to_str().unwrap().to_string(),
            mode: 0o400,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );

        let mut file = File::open(file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "zoo");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o777),
            format!("{:o}", 0o400)
        );
    }
}
