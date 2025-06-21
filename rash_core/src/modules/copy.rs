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
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff_files;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::parse_octal;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{File, OpenOptions, Permissions, metadata, set_permissions};
use std::io::prelude::*;

use std::fmt;
use std::io::Result as IoResult;
use std::io::{BufReader, Write};
use std::os::unix::fs::PermissionsExt;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use tempfile::tempfile;

/// Display permission diff in Ansible-like format
fn diff_permissions(old_mode: u32, new_mode: u32) {
    let old_octal = format!("{:04o}", old_mode & 0o7777);
    let new_octal = format!("{:04o}", new_mode & 0o7777);

    let before = format!("mode={}", old_octal);
    let after = format!("mode={}", new_octal);

    diff_files(&before, &after);
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(flatten)]
    pub input: Input,
    /// The absolute path where the file should be copied to.
    pub dest: String,
    /// Permissions of the destination file or directory.
    /// The mode may also be the special string `preserve`.
    /// `preserve` means that the file will be given the same permissions as the source file.
    pub mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
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

fn change_permissions(
    dest: &str,
    dest_permissions: Permissions,
    mode: u32,
    check_mode: bool,
) -> Result<bool> {
    let masked_mode = mode & 0o7777;
    let current_mode = dest_permissions.mode() & 0o7777;

    // & 0o7777 to remove lead 100: 100644 -> 644
    if current_mode != masked_mode {
        // Show permission diff
        diff_permissions(dest_permissions.mode(), mode);

        if !check_mode {
            trace!("changing mode: {:o}", mode);
            let mut dest_permissions_copy = dest_permissions;
            dest_permissions_copy.set_mode(mode);
            set_permissions(dest, dest_permissions_copy)?;
        }
        return Ok(true);
    }
    Ok(false)
}

#[derive(Debug, PartialEq)]
enum Content {
    Str(String),
    Bytes(Vec<u8>),
}

impl fmt::Display for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Content::Str(s) => write!(f, "{}", s),
            Content::Bytes(_) => Ok(()),
        }
    }
}

impl Content {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Content::Str(s) => s.as_bytes(),
            Content::Bytes(b) => b,
        }
    }

    fn len(&self) -> usize {
        match self {
            Content::Str(s) => s.len(),
            Content::Bytes(b) => b.len(),
        }
    }
}

fn read_content<R: BufRead + Seek>(buf_reader: &mut R) -> IoResult<Content> {
    let mut content = Vec::new();
    buf_reader.read_to_end(&mut content)?;

    match String::from_utf8(content.clone()) {
        Ok(s) => Ok(Content::Str(s)),
        Err(_) => Ok(Content::Bytes(content)),
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
            tempfile()
        }
    })?;
    let mut buf_reader = BufReader::new(&read_file);
    let content = read_content(&mut buf_reader)?;
    let dest_metadata = read_file.metadata()?;
    let dest_permissions = dest_metadata.permissions();
    let mut changed = false;

    let desired_content = match params.input.clone() {
        Input::Content(s) => Content::Str(s),
        Input::Src(src) => {
            let file = File::open(src)?;
            let mut buf_reader = BufReader::new(file);
            read_content(&mut buf_reader)?
        }
    };

    if content != desired_content {
        diff_files(&content, &desired_content);

        if !check_mode {
            trace!("changing content: {:?}", &desired_content);
            if dest_permissions.readonly() {
                let mut p = dest_permissions.clone();
                // enable write
                p.set_mode(dest_permissions.mode() | 0o200);
                set_permissions(&params.dest, p)?;
            }

            let mut file = OpenOptions::new().write(true).open(&params.dest)?;
            file.rewind()?;
            file.write_all(desired_content.as_bytes())?;
            file.set_len(desired_content.len() as u64)?;

            if dest_permissions.readonly() {
                set_permissions(&params.dest, dest_permissions.clone())?;
            }
        }

        changed = true;
    };

    match params.mode.as_deref() {
        Some("preserve") => match params.input {
            Input::Src(src) => {
                let src_metadata = metadata(src)?;
                let src_permissions = src_metadata.permissions();

                changed |= change_permissions(
                    &params.dest,
                    dest_permissions,
                    src_permissions.mode(),
                    check_mode,
                )?;
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "preserve cannot be used in with content",
                ));
            }
        },
        Some(s) => {
            let mode = parse_octal(s)?;
            changed |= change_permissions(&params.dest, dest_permissions, mode, check_mode)?;
        }
        None => (),
    };

    Ok(ModuleResult {
        changed,
        output: Some(params.dest),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Copy;

impl Module for Copy {
    fn get_name(&self) -> &str {
        "copy"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((copy_file(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::error::ErrorKind;

    use std::fs::{File, metadata};
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            content: "boo"
            dest: "/tmp/buu.txt"
            mode: "0600"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Content("boo".to_owned()),
                dest: "/tmp/buu.txt".to_owned(),
                mode: Some("0600".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_mode_int() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            content: "boo"
            dest: "/tmp/buu.txt"
            mode: 0600
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Content("boo".to_owned()),
                dest: "/tmp/buu.txt".to_owned(),
                mode: Some("0600".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_no_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            content: "boo"
            dest: "/tmp/buu.txt"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Content("boo".to_owned()),
                dest: "/tmp/buu.txt".to_owned(),
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_src_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: "/tmp/a"
            dest: "/tmp/buu.txt"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                input: Input::Src("/tmp/a".to_owned()),
                dest: "/tmp/buu.txt".to_owned(),
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_content_and_src() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            content: "boo"
            src: "/tmp/a"
            dest: "/tmp/buu.txt"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            random: "boo"
            src: "/tmp/a"
            dest: "/tmp/buu.txt"
            "#,
        )
        .unwrap();
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
                input: Input::Content("test\n".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_preserve() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let file_src_path = src_dir.path().join("preserve.txt");
        let file_dest_path = dest_dir.path().join("preserve.txt");
        let mut file = File::create(file_src_path.clone()).unwrap();
        writeln!(file, "test").unwrap();

        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o604);
        set_permissions(&file_src_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Src(file_src_path.to_str().unwrap().to_owned()),
                dest: file_dest_path.to_str().unwrap().to_owned(),
                mode: Some("preserve".to_owned()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_dest_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "test\n");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o604)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_dest_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_preserve_with_st_mode_no_change() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let file_src_path = src_dir.path().join("preserve.txt");
        let file_dest_path = dest_dir.path().join("preserve.txt");
        let mut file = File::create(file_src_path.clone()).unwrap();
        writeln!(file, "test").unwrap();

        let mut dest_file = File::create(file_dest_path.clone()).unwrap();
        writeln!(dest_file, "test").unwrap();

        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o100604);
        set_permissions(&file_src_path, permissions).unwrap();

        let mut permissions = dest_file.metadata().unwrap().permissions();
        permissions.set_mode(0o100604);
        set_permissions(&file_dest_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Src(file_src_path.to_str().unwrap().to_owned()),
                dest: file_dest_path.to_str().unwrap().to_owned(),
                mode: Some("preserve".to_owned()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_dest_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "test\n");

        let metadata = file.metadata().unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o604)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: Some(file_dest_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("fu".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("fu".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("zoo".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("zoo".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
            },
            true,
        )
        .unwrap();

        let file_metadata = metadata(&file_path);
        assert!(file_metadata.is_err());
        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("zoo".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0600".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("zoo".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0600".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("zoo".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    // st_mode: https://linux.die.net/man/2/stat
    // bits in octal indicating S_ISREG (if is a file: 100). E.g.: 100644
    fn test_copy_file_read_only_no_change_permissions_check_ignore_st_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("read_only.txt");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(file, "zoo").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o100400);
        set_permissions(&file_path, permissions).unwrap();

        let output = copy_file(
            Params {
                input: Input::Content("zoo\n".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
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
                changed: false,
                output: Some(file_path.to_str().unwrap().to_owned()),
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
                input: Input::Content("zoo".to_owned()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
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
                output: Some(file_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_copy_file_binary() {
        let dir = tempdir().unwrap();

        let src_path = dir.path().join("image.png");
        let file_path = dir.path().join("output_image.png");

        let image_data: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x05, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x8d, 0x6f, 0x26, 0xe5, 0x00, 0x00, 0x00, 0x1c, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xd7, 0x63, 0xf8,
        ];
        let mut file = File::create(src_path.clone()).unwrap();
        file.write_all(image_data).unwrap();
        let output = copy_file(
            Params {
                input: Input::Src(src_path.into_os_string().into_string().unwrap()),
                dest: file_path.to_str().unwrap().to_owned(),
                mode: Some("0400".to_owned()),
            },
            false,
        )
        .unwrap();

        let mut file = File::open(&file_path).unwrap();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();
        assert_eq!(contents, image_data);

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
                output: Some(file_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }
}
