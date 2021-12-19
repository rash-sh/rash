/// ANCHOR: module
/// # find
///
/// Return a list of files based on specific criteria.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - find:
///     paths: /var/log
///     file_type: file
///   register: find_result
///
/// - command: echo "{{ find_result.extra }}"
///
/// - find:
///     paths: /var/log
///     recurse: no
///     file_type: directory
///     excludes: "nginx,mysql"
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, ModuleResult};
use crate::vars::Vars;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use byte_unit::Byte;
use ignore::WalkBuilder;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde_with::{serde_as, OneOrMany};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};
use yaml_rust::Yaml;

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// List of absolute paths of directories to search.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    paths: Vec<String>,
    /// Items whose basenames match an excludes pattern are culled from patterns matches.
    #[serde_as(deserialize_as = "Option<OneOrMany<_>>")]
    #[serde(default)]
    excludes: Option<Vec<String>>,
    /// Type of file to select.
    #[serde(default = "default_file_type")]
    file_type: Option<FileType>,
    /// Set this to true to follow symlinks
    #[serde(default = "default_false")]
    follow: Option<bool>,
    /// If target is a directory, recursively descend into the directory looking for files.
    #[serde(default = "default_false")]
    recurse: Option<bool>,
    /// Select files whose size is less than the specified size.
    /// Unqualified values are in bytes but B, KB, MB, GB, TB can be appended to specify bytes.
    /// KiB, MiB, GiB, TiB can be used too an represent binary values: 1 GiB = 1024 MiB.
    /// Size is not evaluated for directories.
    size: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum FileType {
    Any,
    Directory,
    File,
    Link,
}

fn default_false() -> Option<bool> {
    Some(false)
}

fn default_file_type() -> Option<FileType> {
    Some(FileType::File)
}

fn find(params: Params) -> Result<ModuleResult> {
    let mut walk_builder = WalkBuilder::new(params.paths.first().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "paths must contain at least one valid path",
        )
    })?);
    params.paths.into_iter().skip(1).for_each(|path| {
        walk_builder.add(path);
    });

    if let Some(s) = params.size {
        walk_builder.max_filesize(Some(
            u64::try_from(
                Byte::from_str(&s)
                    .map_err(|_| {
                        Error::new(
                            ErrorKind::InvalidData,
                            "Unable to convert size from string.",
                        )
                    })?
                    .get_bytes(),
            )
            .map_err(|_| {
                Error::new(ErrorKind::InvalidData, "Size overflow, it must feet in u64")
            })?,
        ));
    };

    let result: Vec<String> = walk_builder
        // safe unwrap: default value defined
        .max_depth(match params.recurse.unwrap() {
            false => Some(1),
            true => None,
        })
        // safe unwrap: default value defined
        .follow_links(params.follow.unwrap())
        // this prevents about unbounded feedback loops
        .skip_stdout(true)
        //.hidden(true)//default true: ignore hidden
        //.max_filesize(Some(300))//bytes
        // see ignore files
        .build()
        .into_iter()
        .map(|dir_entry| dir_entry.map_err(|e| Error::new(ErrorKind::Other, e)))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        // safe unwrap: default value defined
        .filter(|x| match params.file_type.as_ref().unwrap() {
            FileType::File => match x.file_type() {
                Some(t) => t.is_file(),
                None => false,
            },
            FileType::Directory => match x.file_type() {
                Some(t) => t.is_dir(),
                None => false,
            },
            FileType::Link => match x.file_type() {
                Some(t) => t.is_symlink(),
                None => false,
            },
            FileType::Any => true,
        })
        .map(|x| match x.path().to_str() {
            Some(s) => Ok(s.to_string()),
            None => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Path `{:?}` cannot be represented as UTF-8", x),
            )),
        })
        .collect::<Result<_>>()?;

    Ok(ModuleResult {
        changed: false,
        output: None,
        extra: Some(json!(result)),
    })
}

pub fn exec(optional_params: Yaml, vars: Vars, _check_mode: bool) -> Result<(ModuleResult, Vars)> {
    Ok((find(parse_params(optional_params)?)?, vars))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::{create_dir, File};

    use tempfile::tempdir;
    use yaml_rust::YamlLoader;

    #[test]
    fn test_parse_params() {
        let yaml = YamlLoader::load_from_str(
            r#"
paths: /var/log
recurse: false
file_type: directory
excludes: 'nginx,mysql'
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
                paths: vec!["/var/log".to_string()],
                file_type: Some(FileType::Directory),
                follow: Some(false),
                excludes: Some(vec!["nginx,mysql".to_string()]),
                recurse: Some(false),
                size: None,
            }
        );
    }

    #[test]
    fn test_parse_params_default() {
        let yaml = YamlLoader::load_from_str(
            r#"
paths: /var/log
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
                paths: vec!["/var/log".to_string()],
                file_type: Some(FileType::File),
                follow: Some(false),
                excludes: None,
                recurse: Some(false),
                size: None,
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml = YamlLoader::load_from_str(
            r#"
paths: /var/log
yea: boo
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
    fn test_parse_params_one_or_many() {
        let yaml = YamlLoader::load_from_str(
            r#"
paths:
  - /foo
  - /boo
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
                paths: vec!["/foo".to_string(), "/boo".to_string()],
                file_type: Some(FileType::File),
                follow: Some(false),
                excludes: None,
                recurse: Some(false),
                size: None,
            }
        );
    }

    #[test]
    fn test_find() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("yea");
        let _ = File::create(file_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_string()],
            file_type: Some(FileType::File),
            follow: Some(false),
            excludes: None,
            recurse: Some(false),
            size: None,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(json!(vec![file_path.to_str().unwrap().to_string()])),
            }
        );
    }

    #[test]
    fn test_find_directories() {
        let dir = tempdir().unwrap();

        let dir_path = dir.path().join("yea");
        let _ = create_dir(dir_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_string()],
            file_type: Some(FileType::Directory),
            follow: Some(false),
            excludes: None,
            recurse: Some(false),
            size: None,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(json!(vec![
                    dir.path().to_str().unwrap().to_string(),
                    dir_path.to_str().unwrap().to_string()
                ])),
            }
        );
    }

    #[test]
    fn test_find_files_recursively() {
        let dir = tempdir().unwrap();

        let dir_path = dir.path().join("child");
        create_dir(dir_path.clone()).unwrap();
        let file_path = dir_path.join("yea");
        let _ = File::create(file_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_string()],
            file_type: Some(FileType::File),
            follow: Some(false),
            excludes: None,
            recurse: Some(true),
            size: None,
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(json!(vec![file_path.to_str().unwrap().to_string()])),
            }
        );
    }
}
