/// ANCHOR: module
/// # copy
///
/// Copy files to path.
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
/// - copy:
///     content: "supersecret"
///     dest: /tmp/MY_PASSWORD_FILE.txt
///     mode: "0400"
/// ```
/// ANCHOR_END: examples
use crate::error::Result;
use crate::logger::diff_files;
use crate::modules::{parse_params, ModuleResult};
use crate::utils::parse_octal;
use crate::vars::Vars;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{set_permissions, File, OpenOptions};
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::{BufReader, Write};
use std::os::unix::fs::PermissionsExt;

#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};
use tempfile::tempfile;
use yaml_rust::Yaml;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(flatten)]
    pub input: Input,
    /// The absolute path where the file should be copied to.
    pub dest: String,
    /// Permissions of the destination file or directory.
    pub mode: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Input {
    /// When used instead of src, sets the contents of a file directly to the specified value.
    Content(String),
    /// Path to a file should be copied to dest.
    Src(String),
}

impl Params {
    #[cfg(test)]
    pub fn get_content(&self) -> Option<String> {
        match &self.input {
            Input::Content(content) => Some(content.clone()),
            _ => None,
        }
    }
}

pub fn copy_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {:?}", params);
    let open_read_file = OpenOptions::new().read(true).clone();
    let read_file = open_read_file.open(&params.dest).or_else(|_| {
        if !check_mode {
            trace!("file does not exists, create new one: {:?}", &params.dest);
            open_read_file
                .clone()
                .write(true)
                .create(true)
                .open(&params.dest)
        } else {
            // TODO: avoid this logic. Hint: remove tempfile from dependencies if not used anywhere
            tempfile()
        }
    })?;
    let mut buf_reader = BufReader::new(&read_file);
    let mut content = String::new();
    buf_reader.read_to_string(&mut content)?;
    let metadata = read_file.metadata()?;
    let mut permissions = metadata.permissions();
    let mut changed = false;

    let desired_content = match params.input {
        Input::Content(s) => s,
        Input::Src(src) => {
            let file = File::open(&src)?;
            let mut buf_reader = BufReader::new(file);
            let mut contents = String::new();
            buf_reader.read_to_string(&mut contents)?;
            contents
        }
    };

    if content != desired_content {
        diff_files(&content, &desired_content);

        if !check_mode {
            trace!("changing content: {:?}", &desired_content);
            if permissions.readonly() {
                let mut p = permissions.clone();
                // enable write
                p.set_mode(permissions.mode() | 0o200);
                set_permissions(&params.dest, p)?;
            }

            let mut file = OpenOptions::new().write(true).open(&params.dest)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(desired_content.as_bytes())?;
            file.set_len(desired_content.len() as u64)?;

            if permissions.readonly() {
                set_permissions(&params.dest, permissions.clone())?;
            }
        }

        changed = true;
    };

    let mode = match params.mode {
        Some(s) => parse_octal(&s)?,
        None => parse_octal("0644")?,
    };

    // & 0o7777 to remove lead 100: 100644 -> 644
    let original_mode = permissions.mode() & 0o7777;
    if original_mode != mode {
        if !check_mode {
            trace!("changing mode: {:o}", &mode);
            permissions.set_mode(mode);
            set_permissions(&params.dest, permissions)?;
        }
        changed = true;
    };

    Ok(ModuleResult {
        changed,
        output: Some(params.dest),
        extra: None,
    })
}

pub fn exec(optional_params: Yaml, vars: Vars, check_mode: bool) -> Result<(ModuleResult, Vars)> {
    Ok((copy_file(parse_params(optional_params)?, check_mode)?, vars))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::error::ErrorKind;

    use std::fs::{metadata, File};
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
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Content("boo".to_string()),
                dest: "/tmp/buu.txt".to_string(),
                mode: Some("0600".to_string()),
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
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Content("boo".to_string()),
                dest: "/tmp/buu.txt".to_string(),
                mode: Some("600".to_string()),
            }
        );
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
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Content("boo".to_string()),
                dest: "/tmp/buu.txt".to_string(),
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_src_field() {
        let yaml = YamlLoader::load_from_str(
            r#"
        src: "/tmp/a"
        dest: "/tmp/buu.txt"
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Src("/tmp/a".to_string()),
                dest: "/tmp/buu.txt".to_string(),
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_content_and_src() {
        let yaml = YamlLoader::load_from_str(
            r#"
        content: "boo"
        src: "/tmp/a"
        dest: "/tmp/buu.txt"
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml = YamlLoader::load_from_str(
            r#"
        random: "boo"
        src: "/tmp/a"
        dest: "/tmp/buu.txt"
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_copy_file_no_change() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("no_change.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "test").unwrap();

        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o644);
        set_permissions(&file_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Content("test\n".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: None,
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "test\n");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o644)
        );

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
    fn test_copy_file_change() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("change.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "test").unwrap();
        let output = copy_file(
            Params {
                input: Input::Content("fu".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0400".to_string()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "fu");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o400)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_change_check_mode() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("change_check_mode.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "test").unwrap();
        let output = copy_file(
            Params {
                input: Input::Content("fu".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0400".to_string()),
            },
            true,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "test\n");
        assert_ne!(contents, "fu");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_ne!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o400)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_create() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("create.txt");
        let output = copy_file(
            Params {
                input: Input::Content("zoo".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0400".to_string()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "zoo");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o400)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_create_src() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("create.txt");
        let src_path = dir.path().join("src.txt");
        let mut file = File::create(src_path.clone()).unwrap();
        writeln!(file, "zoo").unwrap();
        let output = copy_file(
            Params {
                input: Input::Src(src_path.into_os_string().into_string().unwrap()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0400".to_string()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "zoo\n");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o400)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_create_check_mode() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("create_check_mode.txt");
        let output = copy_file(
            Params {
                input: Input::Content("zoo".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0400".to_string()),
            },
            true,
        )
        .unwrap();

        let file_metadata = metadata(&file_path);
        assert_eq!(file_metadata.is_err(), true);
        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_read_only() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("read_only.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "read_only").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Content("zoo".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0600".to_string()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "zoo");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o600)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_read_only_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("read_only_check_mode.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "read_only").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Content("zoo".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0600".to_string()),
            },
            true,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "read_only\n");
        assert_ne!(contents, "zoo");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o400)
        );
        assert_ne!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o600)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_read_only_no_change_permissions() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("read_only.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "read_only").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Content("zoo".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0400".to_string()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "zoo");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o400)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_read_only_no_change_permissions_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("read_only.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "read_only").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Content("zoo".to_string()),
                dest: file_path.to_str().unwrap().to_string(),
                mode: Some("0400".to_string()),
            },
            true,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "read_only\n");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o400)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_string()),
                extra: None,
            }
        );
    }
}
