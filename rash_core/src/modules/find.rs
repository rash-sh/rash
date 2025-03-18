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
/// - debug:
///     var: "find_result.extra"
///
/// - find:
///     paths: /var/log
///     recurse: no
///     file_type: directory
///     excludes: "nginx,mysql"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_if_json, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;

use byte_unit::Byte;
use ignore::WalkBuilder;
use minijinja::Value;
use regex::RegexSet;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
use serde::Deserialize;
use serde_with::{OneOrMany, serde_as};
use serde_yaml::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum FileType {
    Any,
    Directory,
    #[default]
    File,
    Link,
}

fn default_file_type() -> Option<FileType> {
    Some(FileType::default())
}

#[serde_as]
#[derive(Clone, Debug, PartialEq, Deserialize)]
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
    /// **[default: `"file"`]**
    #[serde(default = "default_file_type")]
    file_type: Option<FileType>,
    /// Set this to true to follow symlinks
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    follow: Option<bool>,
    /// Set this to yes to include hidden files, otherwise they will be ignored.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    hidden: Option<bool>,
    /// The patterns restrict the list of files to be returned to those whose basenames
    /// match at least one of the patterns specified.
    /// Multiple patterns can be specified using a list.
    #[serde_as(deserialize_as = "Option<OneOrMany<_>>")]
    #[serde(default)]
    patterns: Option<Vec<String>>,
    /// If target is a directory, recursively descend into the directory looking for files.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    recurse: Option<bool>,
    /// Select files whose size is less than the specified size.
    /// Unqualified values are in bytes but B, KB, MB, GB, TB can be appended to specify bytes.
    /// KiB, MiB, GiB, TiB can be used too an represent binary values: 1 GiB = 1024 MiB.
    /// Size is not evaluated for directories.
    size: Option<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            paths: Vec::new(),
            excludes: None,
            file_type: Some(FileType::default()),
            follow: Some(false),
            hidden: Some(false),
            patterns: None,
            recurse: Some(false),
            size: None,
        }
    }
}

fn get_regex_set(v: Option<Vec<String>>) -> Result<Option<RegexSet>> {
    match v {
        Some(x) => {
            if !x.is_empty() {
                Ok(Some(
                    RegexSet::new(parse_if_json(x)).map_err(|e| Error::new(ErrorKind::Other, e))?,
                ))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

pub fn find(params: Params) -> Result<ModuleResult> {
    let paths = parse_if_json(params.paths);
    if paths.iter().map(Path::new).any(|x| x.is_relative()) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "paths contains relative path",
        ));
    };

    let mut walk_builder = WalkBuilder::new(paths.first().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "paths must contain at least one valid path",
        )
    })?);
    paths.into_iter().skip(1).for_each(|path| {
        walk_builder.add(path);
    });

    if let Some(s) = params.size {
        walk_builder.max_filesize(Some(
            Byte::parse_str(s, true)
                .map_err(|_| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "Unable to convert size from string.",
                    )
                })?
                .as_u64(),
        ));
    };

    let exclude_set = get_regex_set(params.excludes)?;
    let patterns_set = get_regex_set(params.patterns)?;

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
        // safe unwrap: default value defined
        // hidden criterion is opposite for params than for ignore library
        .hidden(!params.hidden.unwrap())
        .ignore(!params.hidden.unwrap())
        .git_global(!params.hidden.unwrap())
        .git_ignore(!params.hidden.unwrap())
        .git_exclude(!params.hidden.unwrap())
        .build()
        .map(|dir_entry| dir_entry.map_err(|e| Error::new(ErrorKind::Other, e)))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        // safe unwrap: default value defined
        .filter(|dir_entry| match params.file_type.as_ref().unwrap() {
            FileType::File => match dir_entry.file_type() {
                Some(t) => t.is_file(),
                None => false,
            },
            FileType::Directory => match dir_entry.file_type() {
                Some(t) => t.is_dir(),
                None => false,
            },
            FileType::Link => match dir_entry.file_type() {
                Some(t) => t.is_symlink(),
                None => false,
            },
            FileType::Any => true,
        })
        .map(|dir_entry| match dir_entry.path().to_str() {
            Some(s) => Ok(s.to_owned()),
            None => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Path `{dir_entry:?}` cannot be represented as UTF-8"),
            )),
        })
        .collect::<Result<Vec<_>>>()?
        .iter()
        .filter(|s| match exclude_set.as_ref() {
            // safe unwrap: previously checked
            Some(set) => !set.is_match(Path::new(s).file_name().unwrap().to_str().unwrap()),
            None => true,
        })
        .filter(|s| match patterns_set.as_ref() {
            // safe unwrap: previously checked
            Some(set) => set.is_match(Path::new(s).file_name().unwrap().to_str().unwrap()),
            None => true,
        })
        .map(String::from)
        .collect();

    Ok(ModuleResult {
        changed: false,
        output: None,
        extra: Some(value::to_value(json!(result))?),
    })
}

#[derive(Debug)]
pub struct Find;

impl Module for Find {
    fn get_name(&self) -> &str {
        "find"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        vars: Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        Ok((find(parse_params(optional_params)?)?, vars))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<RootSchema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::{File, create_dir};
    use std::io::Write;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            paths: /var/log
            recurse: false
            file_type: directory
            excludes: 'nginx,mysql'
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                paths: vec!["/var/log".to_owned()],
                file_type: Some(FileType::Directory),
                recurse: Some(false),
                excludes: Some(vec!["nginx,mysql".to_owned()]),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            paths: /var/log
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                paths: vec!["/var/log".to_owned()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            paths: /var/log
            yea: boo
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_one_or_many() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            paths:
              - /foo
              - /boo
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                paths: vec!["/foo".to_owned(), "/boo".to_owned()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_find() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("yea");
        let _ = File::create(file_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![file_path.to_str().unwrap().to_owned()])).unwrap()
                ),
            }
        );
    }

    #[test]
    fn test_find_json_paths() {
        let dir = tempdir().unwrap();

        let subdir_path1 = dir.path().join("subdir1");
        create_dir(subdir_path1.clone()).unwrap();

        let subdir_path2 = dir.path().join("subdir2");
        create_dir(subdir_path2.clone()).unwrap();

        let subdir_path3 = dir.path().join("subdir3");
        create_dir(subdir_path3.clone()).unwrap();

        let output = find(Params {
            paths: vec![
                format!(
                    r#"["{base_dir}/subdir1", "{base_dir}/subdir2"]"#,
                    base_dir = dir.path().to_str().unwrap()
                ),
                format!(
                    "{base_dir}/subdir3",
                    base_dir = dir.path().to_str().unwrap()
                ),
            ],
            file_type: Some(FileType::Directory),
            ..Default::default()
        })
        .unwrap();

        let mut finds = output
            .extra
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect::<Vec<String>>();
        finds.sort();

        assert_eq!(
            finds,
            vec![
                subdir_path1.to_str().unwrap().to_owned(),
                subdir_path2.to_str().unwrap().to_owned(),
                subdir_path3.to_str().unwrap().to_owned(),
            ],
        );
    }

    #[test]
    fn test_find_relative_path() {
        let error = find(Params {
            paths: vec!["./".to_owned()],
            ..Default::default()
        })
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_find_directories() {
        let dir = tempdir().unwrap();

        let dir_path = dir.path().join("yea");
        create_dir(dir_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            file_type: Some(FileType::Directory),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![
                        dir.path().to_str().unwrap().to_owned(),
                        dir_path.to_str().unwrap().to_owned(),
                    ]))
                    .unwrap()
                ),
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
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            file_type: Some(FileType::File),
            recurse: Some(true),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![file_path.to_str().unwrap().to_owned()])).unwrap()
                ),
            }
        );
    }

    #[test]
    fn test_find_files_ignore_hidden() {
        let dir = tempdir().unwrap();

        let ignore_path = dir.path().join(".ignore");
        let mut ignore_file = File::create(ignore_path).unwrap();
        writeln!(ignore_file, "ignored_file").unwrap();
        let file_path = dir.path().join("ignored_file");
        let _ = File::create(file_path).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            ..Default::default()
        })
        .unwrap();

        let result: Vec<String> = Vec::new();
        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(value::to_value(json!(result)).unwrap()),
            }
        );
    }

    #[test]
    fn test_find_files_hidden_true() {
        let dir = tempdir().unwrap();

        let ignore_path = dir.path().join(".ignore");
        let mut ignore_file = File::create(ignore_path.clone()).unwrap();
        writeln!(ignore_file, "ignored_file").unwrap();
        let file_path = dir.path().join("ignored_file");
        let _ = File::create(file_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            hidden: Some(true),
            ..Default::default()
        })
        .unwrap();

        let mut finds = output
            .extra
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect::<Vec<String>>();
        finds.sort();
        assert_eq!(
            finds,
            vec![
                ignore_path.to_str().unwrap().to_owned(),
                file_path.to_str().unwrap().to_owned(),
            ],
        );
    }

    #[test]
    fn test_find_files_excludes() {
        let dir = tempdir().unwrap();

        let ignore_path = dir.path().join(".ignore");
        let mut ignore_file = File::create(ignore_path).unwrap();
        writeln!(ignore_file, "ignored_file").unwrap();
        let file_path = dir.path().join("ignored_file");
        let _ = File::create(file_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            hidden: Some(true),
            excludes: Some(vec!["\\..*".to_owned()]),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![file_path.to_str().unwrap().to_owned(),])).unwrap()
                ),
            }
        );
    }

    #[test]
    fn test_find_files_excludes_name() {
        let dir = tempdir().unwrap();

        let ignore_path = dir.path().join(".ignore");
        let mut ignore_file = File::create(ignore_path.clone()).unwrap();
        writeln!(ignore_file, "ignored_file").unwrap();
        let file_path = dir.path().join("ignored_file");
        let _ = File::create(file_path).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            hidden: Some(true),
            excludes: Some(vec!["ignored_file".to_owned()]),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![ignore_path.to_str().unwrap().to_owned(),]))
                        .unwrap()
                ),
            }
        );
    }

    #[test]
    fn test_find_directories_exclude() {
        let dir = tempdir().unwrap();
        let parent_path = dir.path().join("foo");
        create_dir(parent_path.clone()).unwrap();

        let dir_path = parent_path.join("boo");
        create_dir(dir_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![parent_path.to_str().unwrap().to_owned()],
            file_type: Some(FileType::Directory),
            excludes: Some(vec!["foo".to_owned()]),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![dir_path.to_str().unwrap().to_owned(),])).unwrap()
                ),
            }
        );
    }

    #[test]
    fn test_find_directories_exclude_from_json() {
        let dir = tempdir().unwrap();
        let parent_path = dir.path().join("foo");
        create_dir(parent_path.clone()).unwrap();

        let dir_path = parent_path.join("boo");
        create_dir(dir_path).unwrap();

        let output = find(Params {
            paths: vec![parent_path.to_str().unwrap().to_owned()],
            file_type: Some(FileType::Directory),
            excludes: Some(vec![r#"["foo", "boo"]"#.to_owned()]),
            ..Default::default()
        })
        .unwrap();

        let result: Vec<String> = Vec::new();
        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(value::to_value(json!(result)).unwrap()),
            }
        );
    }

    #[test]
    fn test_find_patterns() {
        let dir = tempdir().unwrap();
        let file1_path = dir.path().join("file1.txt");
        let _ = File::create(file1_path).unwrap();
        let file2_path = dir.path().join("file2.log");
        let _ = File::create(file2_path.clone()).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            file_type: Some(FileType::File),
            patterns: Some(vec![r".*\.log".to_owned()]),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![file2_path.to_str().unwrap().to_owned(),])).unwrap()
                ),
            }
        );
    }

    #[test]
    fn test_find_patterns_from_json() {
        let dir = tempdir().unwrap();
        let file1_path = dir.path().join("file1.txt");
        let _ = File::create(file1_path).unwrap();
        let file2_path = dir.path().join("file2.log");
        let _ = File::create(&file2_path).unwrap();
        let file3_path = dir.path().join("file3.log");
        let _ = File::create(&file3_path).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            file_type: Some(FileType::File),
            patterns: Some(vec![r#"["file2.log"]"#.to_owned(), "file3.log".to_owned()]),
            ..Default::default()
        })
        .unwrap();

        let mut finds = output
            .extra
            .unwrap()
            .as_sequence()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect::<Vec<String>>();
        finds.sort();

        assert_eq!(
            finds,
            vec![
                file2_path.to_str().unwrap().to_owned(),
                file3_path.to_str().unwrap().to_owned(),
            ],
        );
    }

    #[test]
    fn test_find_directories_patterns() {
        let dir = tempdir().unwrap();
        let parent_path = dir.path().join("foo");
        create_dir(parent_path.clone()).unwrap();

        let dir_path = parent_path.join("boo");
        create_dir(dir_path).unwrap();

        let output = find(Params {
            paths: vec![parent_path.to_str().unwrap().to_owned()],
            file_type: Some(FileType::Directory),
            patterns: Some(vec!["foo".to_owned()]),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![parent_path.to_str().unwrap().to_owned(),]))
                        .unwrap()
                ),
            }
        );
    }

    #[test]
    fn test_find_size() {
        let dir = tempdir().unwrap();

        let file1_path = dir.path().join("file1.txt");
        let _ = File::create(&file1_path).unwrap();

        let output = find(Params {
            paths: vec![dir.path().to_str().unwrap().to_owned()],
            file_type: Some(FileType::File),
            size: Some("20 MB".to_owned()),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            output,
            ModuleResult {
                changed: false,
                output: None,
                extra: Some(
                    value::to_value(json!(vec![file1_path.to_str().unwrap().to_owned(),])).unwrap()
                ),
            }
        );
    }
}
