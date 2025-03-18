/// ANCHOR: module
/// # file
///
/// Manage files and file properties.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - file:
///     path: /work
///     state: absent
///
/// - file:
///     path: /yea
///     mode: 0644
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::parse_octal;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{
    File as StdFile, Metadata, create_dir_all, metadata, remove_dir_all, remove_file,
    set_permissions,
};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
use serde::Deserialize;
use serde_yaml::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Permissions of the destination file or directory.
    mode: Option<String>,
    /// Absolute path to the file being managed.
    path: String,
    /// If _absent_, directories will be recursively deleted, and files or symlinks will be unlinked.
    /// If _directory_, all intermediate subdirectories will be created if they do not exist.
    /// If _file_, with no other options, returns the current state of path.
    /// If _file_, even with other options (such as mode), the file will be modified if it exists but
    ///  will NOT be created if it does not exist.
    /// If _touch_, an empty file will be created if the file does not exist.
    /// **[default: `"file"`]**
    state: Option<State>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    Directory,
    File,
    Touch,
}

fn fail_if_not_exist(params: Params) -> Result<ModuleResult> {
    match metadata(&params.path) {
        Ok(_) => Ok(ModuleResult {
            changed: false,
            output: Some(params.path),
            extra: None,
        }),
        Err(_) => Err(Error::new(
            ErrorKind::NotFound,
            format!("file {} is absent, cannot continue", &params.path),
        )),
    }
}

fn apply_permissions_if_necessary(
    meta: Metadata,
    octal_mode: u32,
    params: Params,
    check_mode: bool,
) -> Result<ModuleResult> {
    let mut permissions = meta.permissions();
    // & 0o7777 to remove lead 100: 100644 -> 644
    let original_mode = permissions.mode() & 0o7777;
    match original_mode != octal_mode {
        true => {
            diff(
                format!("mode: {:o}", &original_mode),
                format!("mode: {:o}", &octal_mode),
            );
            if !check_mode {
                permissions.set_mode(octal_mode);
                set_permissions(&params.path, permissions)?;
            }
            Ok(ModuleResult {
                changed: true,
                output: Some(params.path),
                extra: None,
            })
        }
        false => Ok(ModuleResult {
            changed: false,
            output: Some(params.path),
            extra: None,
        }),
    }
}

fn find_first_existing_directory(path: &Path) -> Result<&Path> {
    match path.is_dir() {
        true => Ok(path),
        false => find_first_existing_directory(path.parent().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Parent of {} cannot be accessed",
                    path.to_str().unwrap_or("Parent cannot be accessed")
                ),
            )
        })?),
    }
}

fn apply_permissions_recursively(octal_mode: u32, path: &Path, until: &Path) -> Result<()> {
    match path == until {
        true => Ok(()),
        false => {
            let meta = metadata(path)?;
            let mut permissions = meta.permissions();
            permissions.set_mode(octal_mode);
            set_permissions(path, permissions)?;
            apply_permissions_recursively(
                octal_mode,
                path.parent().ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "Parent of {} cannot be accessed",
                            path.to_str().unwrap_or("Parent cannot be accessed")
                        ),
                    )
                })?,
                until,
            )
        }
    }
}

fn define_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match &params.state {
        Some(State::File) | None => match &params.mode {
            Some(mode) => {
                let octal_mode = parse_octal(mode)?;
                match metadata(&params.path) {
                    Ok(meta) => {
                        apply_permissions_if_necessary(meta, octal_mode, params, check_mode)
                    }
                    Err(_not_exists) => fail_if_not_exist(params),
                }
            }
            None => fail_if_not_exist(params),
        },
        Some(State::Absent) => match metadata(&params.path) {
            Ok(meta) => {
                if meta.is_file() {
                    diff("state: file\n", "state: absent\n");
                    // add support for symlinks: API is stable now
                    // if meta.is_file() || meta.is_symlink() {
                    if !check_mode {
                        remove_file(&params.path)?;
                    }
                } else if meta.is_dir() {
                    diff("state: directory\n", "state: absent\n");
                    if !check_mode {
                        remove_dir_all(&params.path)?;
                    }
                } else {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "file {} is unknown type and cannot be removed",
                            &params.path
                        ),
                    ));
                }
                Ok(ModuleResult {
                    changed: true,
                    output: Some(params.path),
                    extra: None,
                })
            }
            Err(_not_exists) => Ok(ModuleResult {
                changed: false,
                output: Some(params.path),
                extra: None,
            }),
        },
        Some(State::Directory) => match &params.mode {
            Some(mode) => {
                let octal_mode = parse_octal(mode)?;
                match metadata(&params.path) {
                    Ok(meta) => {
                        apply_permissions_if_necessary(meta, octal_mode, params, check_mode)
                    }
                    Err(_not_exists) => {
                        diff("state: absent\n", "state: directory\n");

                        if !check_mode {
                            let first_existing_parent =
                                find_first_existing_directory(Path::new(&params.path))?;
                            create_dir_all(&params.path)?;
                            apply_permissions_recursively(
                                octal_mode,
                                Path::new(&params.path),
                                first_existing_parent,
                            )?;
                        }

                        Ok(ModuleResult {
                            changed: true,
                            output: Some(params.path),
                            extra: None,
                        })
                    }
                }
            }
            None => match metadata(&params.path) {
                Ok(_exists) => Ok(ModuleResult {
                    changed: false,
                    output: Some(params.path),
                    extra: None,
                }),
                Err(_not_exists) => {
                    diff("state: absent\n", "state: directory\n");

                    if !check_mode {
                        create_dir_all(&params.path)?;
                    }

                    Ok(ModuleResult {
                        changed: true,
                        output: Some(params.path),
                        extra: None,
                    })
                }
            },
        },
        Some(State::Touch) => match &params.mode {
            Some(mode) => {
                let octal_mode = parse_octal(mode)?;
                match metadata(&params.path) {
                    Ok(meta) => {
                        apply_permissions_if_necessary(meta, octal_mode, params, check_mode)
                    }
                    Err(_not_exists) => {
                        diff("state: absent\n", "state: file\n");

                        if !check_mode {
                            let file = StdFile::create(&params.path)?;
                            let mut permissions = file.metadata()?.permissions();
                            permissions.set_mode(octal_mode);
                            set_permissions(&params.path, permissions)?;
                        }

                        Ok(ModuleResult {
                            changed: true,
                            output: Some(params.path),
                            extra: None,
                        })
                    }
                }
            }
            None => match metadata(&params.path) {
                Ok(_) => Ok(ModuleResult {
                    changed: false,
                    output: Some(params.path),
                    extra: None,
                }),
                Err(_not_exists) => {
                    diff("state: absent\n", "state: file\n");
                    if !check_mode {
                        StdFile::create(&params.path)?;
                    }

                    Ok(ModuleResult {
                        changed: true,
                        output: Some(params.path),
                        extra: None,
                    })
                }
            },
        },
    }
}

#[derive(Debug)]
pub struct File;

impl Module for File {
    fn get_name(&self) -> &str {
        "file"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        vars: Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        Ok((
            define_file(parse_params(optional_params)?, check_mode)?,
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

    use std::fs::create_dir;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            path: /yea
            state: file
            mode: "0644"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                mode: Some("0644".to_owned()),
                path: "/yea".to_owned(),
                state: Some(State::File),
            }
        );
    }

    #[test]
    fn test_parse_params_no_path() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            mode: "0644"
            state: file
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_no_mode() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            path: foo
            state: directory
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                mode: None,
                path: "foo".to_owned(),
                state: Some(State::Directory),
            }
        );
    }

    #[test]
    fn test_parse_params_invalid_mode() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            mode:
              yea: boo
            path: foo
            state: directory
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_no_state() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            path: foo
        "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                mode: None,
                path: "foo".to_owned(),
                state: None,
            }
        );
    }

    #[test]
    fn test_define_file_no_change() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("no_change");
        let file = StdFile::create(file_path.clone()).unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: file_path.to_str().unwrap().to_owned(),
                state: None,
                mode: None,
            },
            false,
        )
        .unwrap();

        let permissions = metadata(&file_path).unwrap().permissions();
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
    fn test_define_file_no_exists() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("no_exists");
        let error = define_file(
            Params {
                path: file_path.to_str().unwrap().to_owned(),
                state: None,
                mode: None,
            },
            false,
        )
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_define_file_created() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().join("created");

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Touch),
                mode: None,
            },
            false,
        )
        .unwrap();

        let dir_metadata = metadata(&dir_path).unwrap();
        let dir_permissions = dir_metadata.permissions();
        assert!(dir_metadata.is_file());
        assert_eq!(
            format!("{:o}", dir_permissions.mode() & 0o7777),
            format!("{:o}", 0o644)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_define_file_created_check_mode() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().join("created");

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Touch),
                mode: None,
            },
            true,
        )
        .unwrap();

        let dir_metadata = metadata(&dir_path);
        assert!(dir_metadata.is_err());
        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_define_file_created_directory_and_subdirectories() {
        let dir = tempdir().unwrap();
        let parent_path = dir.path().join("parent");
        let dir_path = parent_path
            .join("foo")
            .join("created_directory_and_subdirectories");

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Directory),
                mode: Some("0750".to_owned()),
            },
            false,
        )
        .unwrap();
        let parent_metadata = metadata(&parent_path).unwrap();
        let parent_permissions = parent_metadata.permissions();
        assert!(parent_metadata.is_dir());
        assert_eq!(
            format!("{:o}", parent_permissions.mode() & 0o7777),
            format!("{:o}", 0o750)
        );

        let dir_metadata = metadata(&dir_path).unwrap();
        let dir_permissions = dir_metadata.permissions();
        assert!(dir_metadata.is_dir());
        assert_eq!(
            format!("{:o}", dir_permissions.mode() & 0o7777),
            format!("{:o}", 0o750)
        );

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_define_file_created_directory_and_subdirectories_check_mode() {
        let dir = tempdir().unwrap();
        let parent_path = dir.path().join("parent");
        let dir_path = parent_path
            .join("foo")
            .join("created_directory_and_subdirectories");

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Directory),
                mode: Some("0750".to_owned()),
            },
            true,
        )
        .unwrap();

        let parent_metadata = metadata(&parent_path);
        let dir_metadata = metadata(&dir_path);
        assert!(parent_metadata.is_err());
        assert!(dir_metadata.is_err());

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_define_file_modify_permissions() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("modify_permissions");
        let file = StdFile::create(file_path.clone()).unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: file_path.to_str().unwrap().to_owned(),
                state: Some(State::File),
                mode: Some("0604".to_owned()),
            },
            false,
        )
        .unwrap();

        let file_metadata = metadata(&file_path).unwrap();
        let permissions = file_metadata.permissions();
        assert!(file_metadata.is_file());
        assert_eq!(
            format!("{:o}", permissions.mode() & 0o7777),
            format!("{:o}", 0o604)
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
    fn test_define_file_modify_permissions_check_mode() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("modify_permissions");
        let file = StdFile::create(file_path.clone()).unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: file_path.to_str().unwrap().to_owned(),
                state: Some(State::File),
                mode: Some("0604".to_owned()),
            },
            true,
        )
        .unwrap();

        let file_metadata = metadata(&file_path).unwrap();
        let permissions = file_metadata.permissions();
        assert!(file_metadata.is_file());
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
    fn test_define_file_remove_file() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("remove_file");
        let file = StdFile::create(file_path.clone()).unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: file_path.to_str().unwrap().to_owned(),
                state: Some(State::Absent),
                mode: None,
            },
            false,
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
    fn test_define_file_remove_file_check_mode() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("remove_file");
        let file = StdFile::create(file_path.clone()).unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o400);
        set_permissions(&file_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: file_path.to_str().unwrap().to_owned(),
                state: Some(State::Absent),
                mode: None,
            },
            true,
        )
        .unwrap();

        let file_metadata = metadata(&file_path);
        assert!(file_metadata.is_ok());

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
    fn test_define_file_remove_directory() {
        let dir = tempdir().unwrap();

        let dir_path = dir.path().join("remove_directory");
        create_dir(&dir_path).unwrap();
        let dir_metadata = metadata(&dir_path).unwrap();
        let mut permissions = dir_metadata.permissions();
        permissions.set_mode(0o700);
        set_permissions(&dir_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Absent),
                mode: None,
            },
            false,
        )
        .unwrap();

        let dir_metadata = metadata(&dir_path);
        assert!(dir_metadata.is_err());

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_define_file_remove_directory_check_mode() {
        let dir = tempdir().unwrap();

        let dir_path = dir.path().join("remove_directory");
        create_dir(&dir_path).unwrap();
        let dir_metadata = metadata(&dir_path).unwrap();
        let mut permissions = dir_metadata.permissions();
        permissions.set_mode(0o700);
        set_permissions(&dir_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Absent),
                mode: None,
            },
            true,
        )
        .unwrap();

        let dir_metadata = metadata(&dir_path);
        assert!(dir_metadata.is_ok());

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_define_file_remove_directory_and_subdirectories() {
        let dir = tempdir().unwrap();

        let dir_path = dir.path().join("remove_directory_and_subdirectories");
        create_dir(&dir_path).unwrap();
        create_dir(dir_path.join("one_dir")).unwrap();
        StdFile::create(dir_path.join("one_file")).unwrap();
        let dir_metadata = metadata(&dir_path).unwrap();
        let mut permissions = dir_metadata.permissions();
        permissions.set_mode(0o700);
        set_permissions(&dir_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Absent),
                mode: None,
            },
            false,
        )
        .unwrap();

        let dir_metadata = metadata(&dir_path);
        assert!(dir_metadata.is_err());

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }

    #[test]
    fn test_define_file_remove_directory_and_subdirectories_check_mode() {
        let dir = tempdir().unwrap();

        let dir_path = dir.path().join("remove_directory_and_subdirectories");
        create_dir(&dir_path).unwrap();
        create_dir(dir_path.join("one_dir")).unwrap();
        StdFile::create(dir_path.join("one_file")).unwrap();
        let dir_metadata = metadata(&dir_path).unwrap();
        let mut permissions = dir_metadata.permissions();
        permissions.set_mode(0o700);
        set_permissions(&dir_path, permissions).unwrap();

        let output = define_file(
            Params {
                path: dir_path.to_str().unwrap().to_owned(),
                state: Some(State::Absent),
                mode: None,
            },
            true,
        )
        .unwrap();

        let dir_metadata = metadata(&dir_path);
        assert!(dir_metadata.is_ok());

        assert_eq!(
            output,
            ModuleResult {
                changed: true,
                output: Some(dir_path.to_str().unwrap().to_owned()),
                extra: None,
            }
        );
    }
}
