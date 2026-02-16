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

use std::fs::{File, OpenOptions, Permissions, create_dir_all, metadata, set_permissions};
use std::io::prelude::*;

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::Result as IoResult;
use std::io::{BufReader, Write};
use std::os::unix::fs as unix_fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

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

    let before = format!("mode={old_octal}");
    let after = format!("mode={new_octal}");

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
    /// Whether to follow symlinks when copying. If false and the source is a symlink, the symlink itself is copied.
    /// [default: true]
    #[serde(default = "default_dereference")]
    pub dereference: bool,
}

fn default_dereference() -> bool {
    true
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
            trace!("changing mode: {mode:o}");
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
            Content::Str(s) => write!(f, "{s}"),
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

fn copy_symlink(src: &str, dest: &str, check_mode: bool) -> Result<Option<ModuleResult>> {
    let src_path = Path::new(src);
    let dest_path = Path::new(dest);

    let src_meta = fs::symlink_metadata(src_path)?;
    if !src_meta.file_type().is_symlink() {
        return Ok(None);
    }

    let src_target = fs::read_link(src_path)?;

    match fs::symlink_metadata(dest_path) {
        Ok(dest_meta) if dest_meta.file_type().is_symlink() => {
            let dest_target = fs::read_link(dest_path)?;
            if dest_target == src_target {
                return Ok(Some(ModuleResult {
                    changed: false,
                    output: Some(dest.to_owned()),
                    extra: None,
                }));
            }
            diff_files(
                format!("symlink -> {}", dest_target.display()),
                format!("symlink -> {}", src_target.display()),
            );
            if !check_mode {
                fs::remove_file(dest_path)?;
                unix_fs::symlink(&src_target, dest_path)?;
            }
        }
        Ok(_) => {
            diff_files("file", format!("symlink -> {}", src_target.display()));
            if !check_mode {
                fs::remove_file(dest_path)?;
                unix_fs::symlink(&src_target, dest_path)?;
            }
        }
        Err(_) => {
            diff_files("(absent)", format!("symlink -> {}", src_target.display()));
            if !check_mode {
                unix_fs::symlink(&src_target, dest_path)?;
            }
        }
    }

    Ok(Some(ModuleResult {
        changed: true,
        output: Some(dest.to_owned()),
        extra: None,
    }))
}

fn dest_is_directory(dest: &str) -> bool {
    dest.ends_with('/')
}

fn copy_single_file(
    src: &str,
    dest: &str,
    mode: Option<&str>,
    dereference: bool,
    check_mode: bool,
) -> Result<ModuleResult> {
    let params = Params {
        input: Input::Src(src.to_owned()),
        dest: dest.to_owned(),
        mode: mode.map(|m| m.to_owned()),
        dereference,
    };
    copy_file(params, check_mode)
}

fn copy_directory(
    src: &str,
    dest: &str,
    mode: Option<&str>,
    check_mode: bool,
) -> Result<ModuleResult> {
    let src_path = Path::new(src);
    let dest_path = Path::new(dest);

    if !src_path.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("src {} is not a directory", src),
        ));
    }

    let mut changed = false;
    let mut created_dirs = HashSet::new();

    for entry in walkdir::WalkDir::new(src_path).follow_links(true) {
        let entry =
            entry.map_err(|e| Error::new(ErrorKind::IOError, format!("walkdir error: {}", e)))?;
        let src_entry_path = entry.path();
        let relative = src_entry_path.strip_prefix(src_path).map_err(|e| {
            Error::new(ErrorKind::InvalidData, format!("strip prefix error: {}", e))
        })?;
        let dest_entry_path = dest_path.join(relative);

        if entry.file_type().is_dir() {
            if !dest_entry_path.exists() {
                if !check_mode {
                    create_dir_all(&dest_entry_path)?;
                    if let Some(m) = mode {
                        let octal_mode = parse_octal(m)?;
                        let mut perms = fs::metadata(&dest_entry_path)?.permissions();
                        perms.set_mode(octal_mode);
                        set_permissions(&dest_entry_path, perms)?;
                    }
                }
                changed = true;
                created_dirs.insert(dest_entry_path.clone());
            }
        } else if entry.file_type().is_file() {
            let result = copy_single_file(
                src_entry_path.to_str().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in source path")
                })?,
                dest_entry_path.to_str().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in destination path")
                })?,
                mode,
                true,
                check_mode,
            )?;
            if result.changed {
                changed = true;
            }
        } else if entry.file_type().is_symlink() {
            let link_target = fs::read_link(src_entry_path)?;
            match fs::symlink_metadata(&dest_entry_path) {
                Ok(existing) if existing.file_type().is_symlink() => {
                    let existing_target = fs::read_link(&dest_entry_path)?;
                    if existing_target != link_target {
                        if !check_mode {
                            fs::remove_file(&dest_entry_path)?;
                            unix_fs::symlink(&link_target, &dest_entry_path)?;
                        }
                        changed = true;
                    }
                }
                Ok(_) => {
                    if !check_mode {
                        fs::remove_file(&dest_entry_path)?;
                        unix_fs::symlink(&link_target, &dest_entry_path)?;
                    }
                    changed = true;
                }
                Err(_) => {
                    if !check_mode {
                        unix_fs::symlink(&link_target, &dest_entry_path)?;
                    }
                    changed = true;
                }
            }
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(dest.to_owned()),
        extra: None,
    })
}

pub fn copy_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    if let Input::Src(ref src) = params.input {
        let src_path = Path::new(src);

        if src_path.is_dir() {
            let dest = if dest_is_directory(&params.dest) {
                params.dest.trim_end_matches('/').to_owned()
            } else {
                params.dest.clone()
            };
            return copy_directory(src, &dest, params.mode.as_deref(), check_mode);
        }

        if !params.dereference
            && let Some(result) = copy_symlink(src, &params.dest, check_mode)?
        {
            return Ok(result);
        }

        if dest_is_directory(&params.dest) {
            let dest_dir = params.dest.trim_end_matches('/');
            let dest_path = Path::new(dest_dir);
            let src_filename = src_path.file_name().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Cannot extract filename from src: {}", src),
                )
            })?;
            let final_dest = dest_path.join(src_filename);

            if !dest_path.exists() && !check_mode {
                create_dir_all(dest_path)?;
            }

            return copy_single_file(
                src,
                final_dest.to_str().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in destination path")
                })?,
                params.mode.as_deref(),
                params.dereference,
                check_mode,
            );
        }
    }

    if dest_is_directory(&params.dest) && matches!(params.input, Input::Content(_)) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "dest ending with '/' requires src to be a file path, not content",
        ));
    }

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
        Input::Src(ref src) => {
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

    use std::fs::{File, create_dir_all, metadata};
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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
                dereference: true,
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

    #[test]
    fn test_copy_file_symlink_dereference_false() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");
        // Create source file
        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "symlinked").unwrap();
        }
        // Create symlink
        symlink(&src_path, &link_path).unwrap();
        // Copy the symlink itself, not the target
        let output = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        // dest should be a symlink
        let meta = std::fs::symlink_metadata(&dest_path).unwrap();
        assert!(meta.file_type().is_symlink());
        let target = std::fs::read_link(&dest_path).unwrap();
        assert_eq!(target, src_path);
        assert!(output.changed);
    }

    #[test]
    fn test_copy_file_symlink_dereference_true() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");
        // Create source file
        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "symlinked").unwrap();
        }
        // Create symlink
        symlink(&src_path, &link_path).unwrap();
        // Copy the file pointed to by the symlink
        let output = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: true,
            },
            false,
        )
        .unwrap();
        // dest should be a regular file with the same content as src
        let meta = std::fs::symlink_metadata(&dest_path).unwrap();
        assert!(meta.file_type().is_file());
        let mut contents = String::new();
        File::open(&dest_path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "symlinked\n");
        assert!(output.changed);
    }

    #[test]
    fn test_copy_file_symlink_overwrite_existing_file() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");
        // Create source file
        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "symlinked").unwrap();
        }
        symlink(&src_path, &link_path).unwrap();
        {
            let mut file = File::create(&dest_path).unwrap();
            writeln!(file, "old").unwrap();
        }

        let output = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();

        let meta = std::fs::symlink_metadata(&dest_path).unwrap();
        assert!(meta.file_type().is_symlink());
        let target = std::fs::read_link(&dest_path).unwrap();
        assert_eq!(target, src_path);
        assert!(output.changed);
    }

    #[test]
    fn test_copy_file_symlink_idempotent() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");
        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "symlinked").unwrap();
        }
        symlink(&src_path, &link_path).unwrap();

        let output1 = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(output1.changed);

        let output2 = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(!output2.changed);

        let meta = std::fs::symlink_metadata(&dest_path).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(std::fs::read_link(&dest_path).unwrap(), src_path);
    }

    #[test]
    fn test_copy_file_symlink_to_unreadable_file() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");

        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "secret").unwrap();
            let mut permissions = file.metadata().unwrap().permissions();
            permissions.set_mode(0o000);
            set_permissions(&src_path, permissions).unwrap();
        }
        symlink(&src_path, &link_path).unwrap();

        let output = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(output.changed);

        let meta = std::fs::symlink_metadata(&dest_path).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(std::fs::read_link(&dest_path).unwrap(), src_path);

        let mut permissions = std::fs::metadata(&src_path).unwrap().permissions();
        permissions.set_mode(0o644);
        set_permissions(&src_path, permissions).unwrap();
    }

    #[test]
    fn test_copy_file_symlink_idempotent_unreadable_target() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");

        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "secret").unwrap();
            let mut permissions = file.metadata().unwrap().permissions();
            permissions.set_mode(0o000);
            set_permissions(&src_path, permissions).unwrap();
        }
        symlink(&src_path, &link_path).unwrap();

        let output1 = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(output1.changed);

        let output2 = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(!output2.changed);

        let mut permissions = std::fs::metadata(&src_path).unwrap().permissions();
        permissions.set_mode(0o644);
        set_permissions(&src_path, permissions).unwrap();
    }

    #[test]
    fn test_copy_file_symlink_overwrite_different_symlink() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let other_path = dir.path().join("other.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");

        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "source").unwrap();
        }
        {
            let mut file = File::create(&other_path).unwrap();
            writeln!(file, "other").unwrap();
        }

        symlink(&src_path, &link_path).unwrap();
        symlink(&other_path, &dest_path).unwrap();

        let output = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(output.changed);

        let meta = std::fs::symlink_metadata(&dest_path).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(std::fs::read_link(&dest_path).unwrap(), src_path);
    }

    #[test]
    fn test_copy_file_symlink_check_mode() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");
        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "symlinked").unwrap();
        }
        symlink(&src_path, &link_path).unwrap();

        let output = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            true,
        )
        .unwrap();
        assert!(output.changed);
        assert!(!dest_path.exists());
    }

    #[test]
    fn test_copy_file_symlink_idempotent_readonly_target() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.txt");
        let link_path = dir.path().join("link.txt");
        let dest_path = dir.path().join("dest.txt");

        {
            let mut file = File::create(&src_path).unwrap();
            writeln!(file, "readonly content").unwrap();
            let mut permissions = file.metadata().unwrap().permissions();
            permissions.set_mode(0o444);
            set_permissions(&src_path, permissions).unwrap();
        }
        symlink(&src_path, &link_path).unwrap();

        let output1 = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(output1.changed);

        let output2 = copy_file(
            Params {
                input: Input::Src(link_path.to_str().unwrap().to_owned()),
                dest: dest_path.to_str().unwrap().to_owned(),
                mode: None,
                dereference: false,
            },
            false,
        )
        .unwrap();
        assert!(!output2.changed);

        let meta = std::fs::symlink_metadata(&dest_path).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(std::fs::read_link(&dest_path).unwrap(), src_path);
    }

    #[test]
    fn test_copy_file_to_directory_dest() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest = dest_dir.path().join("subdir/");
        let output = copy_file(
            Params {
                input: Input::Src(src_file.to_str().unwrap().to_owned()),
                dest: dest.to_str().unwrap().to_owned(),
                mode: None,
                dereference: true,
            },
            false,
        )
        .unwrap();

        let expected_dest = dest_dir.path().join("subdir").join("test.txt");
        assert!(expected_dest.exists());
        let mut contents = String::new();
        File::open(&expected_dest)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "test content\n");
        assert!(output.changed);
    }

    #[test]
    fn test_copy_file_to_existing_directory_dest() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest_subdir = dest_dir.path().join("subdir");
        create_dir_all(&dest_subdir).unwrap();

        let dest = dest_dir.path().join("subdir/").to_str().unwrap().to_owned();
        let output = copy_file(
            Params {
                input: Input::Src(src_file.to_str().unwrap().to_owned()),
                dest,
                mode: None,
                dereference: true,
            },
            false,
        )
        .unwrap();

        let expected_dest = dest_subdir.join("test.txt");
        assert!(expected_dest.exists());
        let mut contents = String::new();
        File::open(&expected_dest)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "test content\n");
        assert!(output.changed);
    }

    #[test]
    fn test_copy_file_to_directory_dest_check_mode() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest = dest_dir.path().join("subdir/").to_str().unwrap().to_owned();
        let output = copy_file(
            Params {
                input: Input::Src(src_file.to_str().unwrap().to_owned()),
                dest,
                mode: None,
                dereference: true,
            },
            true,
        )
        .unwrap();

        let expected_dest = dest_dir.path().join("subdir").join("test.txt");
        assert!(!expected_dest.exists());
        assert!(output.changed);
    }

    #[test]
    fn test_copy_directory() {
        let src_dir = tempdir().unwrap();
        let dest_parent = tempdir().unwrap();

        let src_file1 = src_dir.path().join("file1.txt");
        let mut file = File::create(&src_file1).unwrap();
        writeln!(file, "content1").unwrap();

        let src_subdir = src_dir.path().join("subdir");
        create_dir_all(&src_subdir).unwrap();
        let src_file2 = src_subdir.join("file2.txt");
        let mut file = File::create(&src_file2).unwrap();
        writeln!(file, "content2").unwrap();

        let dest = dest_parent.path().join("copied_dir");
        let output = copy_file(
            Params {
                input: Input::Src(src_dir.path().to_str().unwrap().to_owned()),
                dest: dest.to_str().unwrap().to_owned(),
                mode: None,
                dereference: true,
            },
            false,
        )
        .unwrap();

        assert!(dest.join("file1.txt").exists());
        assert!(dest.join("subdir/file2.txt").exists());
        let mut contents = String::new();
        File::open(dest.join("file1.txt"))
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "content1\n");
        assert!(output.changed);
    }

    #[test]
    fn test_copy_directory_with_trailing_slash() {
        let src_dir = tempdir().unwrap();
        let dest_parent = tempdir().unwrap();

        let src_file = src_dir.path().join("file.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "content").unwrap();

        let dest = dest_parent.path().join("copied_dir/");
        let output = copy_file(
            Params {
                input: Input::Src(src_dir.path().to_str().unwrap().to_owned()),
                dest: dest.to_str().unwrap().to_owned(),
                mode: None,
                dereference: true,
            },
            false,
        )
        .unwrap();

        assert!(dest_parent.path().join("copied_dir/file.txt").exists());
        assert!(output.changed);
    }

    #[test]
    fn test_copy_directory_check_mode() {
        let src_dir = tempdir().unwrap();
        let dest_parent = tempdir().unwrap();

        let src_file = src_dir.path().join("file.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "content").unwrap();

        let dest = dest_parent.path().join("copied_dir");
        let output = copy_file(
            Params {
                input: Input::Src(src_dir.path().to_str().unwrap().to_owned()),
                dest: dest.to_str().unwrap().to_owned(),
                mode: None,
                dereference: true,
            },
            true,
        )
        .unwrap();

        assert!(!dest.exists());
        assert!(output.changed);
    }

    #[test]
    fn test_copy_directory_with_mode() {
        let src_dir = tempdir().unwrap();
        let dest_parent = tempdir().unwrap();

        let src_file = src_dir.path().join("file.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "content").unwrap();

        let dest = dest_parent.path().join("copied_dir");
        let output = copy_file(
            Params {
                input: Input::Src(src_dir.path().to_str().unwrap().to_owned()),
                dest: dest.to_str().unwrap().to_owned(),
                mode: Some("0755".to_owned()),
                dereference: true,
            },
            false,
        )
        .unwrap();

        let metadata = metadata(dest.join("file.txt")).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o755)
        );
        assert!(output.changed);
    }

    #[test]
    fn test_copy_directory_idempotent() {
        let src_dir = tempdir().unwrap();
        let dest_parent = tempdir().unwrap();

        let src_file = src_dir.path().join("file.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "same content").unwrap();

        let dest = dest_parent.path().join("copied_dir");
        let output1 = copy_file(
            Params {
                input: Input::Src(src_dir.path().to_str().unwrap().to_owned()),
                dest: dest.to_str().unwrap().to_owned(),
                mode: None,
                dereference: true,
            },
            false,
        )
        .unwrap();
        assert!(output1.changed);

        let output2 = copy_file(
            Params {
                input: Input::Src(src_dir.path().to_str().unwrap().to_owned()),
                dest: dest.to_str().unwrap().to_owned(),
                mode: None,
                dereference: true,
            },
            false,
        )
        .unwrap();
        assert!(!output2.changed);
    }

    #[test]
    fn test_copy_content_to_directory_dest_error() {
        let dest_dir = tempdir().unwrap();

        let dest = dest_dir.path().join("subdir/").to_str().unwrap().to_owned();
        let result = copy_file(
            Params {
                input: Input::Content("test content".to_owned()),
                dest,
                mode: None,
                dereference: true,
            },
            false,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
