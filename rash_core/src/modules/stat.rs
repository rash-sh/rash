/// ANCHOR: module
/// # stat
///
/// Retrieve file or file system status.
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
/// - stat:
///     path: /etc/app/config.json
///   register: config_stat
///
/// - name: Only run if config exists and is recent
///   command:
///     cmd: ./reload-config.sh
///   when: config_stat.stat.exists and config_stat.stat.mtime > (ansible_date_time.epoch | int - 86400)
///
/// - stat:
///     path: /path/to/file
///     checksum_algorithm: sha256
///   register: file_stat
///
/// - debug:
///     var: "file_stat.stat.checksum"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, Module, ModuleResult};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, metadata, symlink_metadata};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use md5::Md5;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::{value, Value as YamlValue};
use sha1::Sha1;
use sha2::{Digest, Sha256};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum ChecksumAlgorithm {
    Md5,
    Sha1,
    #[default]
    Sha256,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The full path of the file/object to get the facts of.
    path: String,
    /// Algorithm to determine checksum of file.
    /// **[default: `"sha256"`]**
    checksum_algorithm: Option<ChecksumAlgorithm>,
    /// Whether to follow symlinks.
    /// **[default: `false`]**
    #[serde(default)]
    follow: bool,
    /// Whether to get the checksum of a file.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    get_checksum: bool,
    /// Whether to get the md5 checksum of a file.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    get_md5: bool,
    /// Whether to get the mime type of a file.
    /// Requires file command to be available.
    /// **[default: `false`]**
    #[serde(default)]
    get_mime: bool,
    /// Whether to get the attributes of a file.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    get_attributes: bool,
}

fn default_true() -> bool {
    true
}

fn system_time_to_epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn calculate_checksum(path: &Path, algorithm: &ChecksumAlgorithm) -> Result<String> {
    let contents = fs::read(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read file for checksum: {e}"),
        )
    })?;

    match algorithm {
        ChecksumAlgorithm::Md5 => {
            let mut hasher = Md5::new();
            hasher.update(&contents);
            Ok(format!("{:x}", hasher.finalize()))
        }
        ChecksumAlgorithm::Sha1 => {
            let mut hasher = Sha1::new();
            hasher.update(&contents);
            Ok(format!("{:x}", hasher.finalize()))
        }
        ChecksumAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(&contents);
            Ok(format!("{:x}", hasher.finalize()))
        }
    }
}

fn get_file_stat(path: &str, params: &Params) -> Result<serde_json::Value> {
    let path_obj = Path::new(path);
    let meta_result = if params.follow {
        metadata(path)
    } else {
        symlink_metadata(path)
    };

    let mut stat = serde_json::Map::new();

    match meta_result {
        Ok(meta) => {
            let is_symlink = meta.file_type().is_symlink();

            stat.insert("exists".to_string(), serde_json::json!(true));
            stat.insert("isdir".to_string(), serde_json::json!(meta.is_dir()));
            stat.insert("isfile".to_string(), serde_json::json!(meta.is_file()));
            stat.insert("islnk".to_string(), serde_json::json!(is_symlink));
            stat.insert("issock".to_string(), serde_json::json!(false));
            stat.insert("isblk".to_string(), serde_json::json!(false));
            stat.insert("ischr".to_string(), serde_json::json!(false));
            stat.insert("isfifo".to_string(), serde_json::json!(false));
            stat.insert(
                "isreg".to_string(),
                serde_json::json!(meta.is_file() && !is_symlink),
            );

            let mode = meta.mode() & 0o7777;
            stat.insert(
                "mode".to_string(),
                serde_json::json!(format!("{:04o}", mode)),
            );

            let permissions = meta.permissions();
            let mode_bits = permissions.mode();

            stat.insert(
                "readable".to_string(),
                serde_json::json!((mode_bits & 0o444) != 0),
            );
            stat.insert(
                "writeable".to_string(),
                serde_json::json!((mode_bits & 0o222) != 0),
            );
            stat.insert(
                "executable".to_string(),
                serde_json::json!((mode_bits & 0o111) != 0),
            );

            stat.insert("size".to_string(), serde_json::json!(meta.len()));
            stat.insert("uid".to_string(), serde_json::json!(meta.uid()));
            stat.insert("gid".to_string(), serde_json::json!(meta.gid()));

            stat.insert(
                "atime".to_string(),
                serde_json::json!(system_time_to_epoch(meta.accessed()?)),
            );
            stat.insert(
                "mtime".to_string(),
                serde_json::json!(system_time_to_epoch(meta.modified()?)),
            );
            stat.insert("ctime".to_string(), serde_json::json!(meta.ctime() as u64));

            stat.insert("inode".to_string(), serde_json::json!(meta.ino()));
            stat.insert("dev".to_string(), serde_json::json!(meta.dev()));
            stat.insert("nlink".to_string(), serde_json::json!(meta.nlink()));
            stat.insert("blocks".to_string(), serde_json::json!(meta.blocks()));
            stat.insert("blksize".to_string(), serde_json::json!(meta.blksize()));

            if is_symlink {
                let link_target = fs::read_link(path)
                    .ok()
                    .and_then(|p| p.to_str().map(String::from));
                stat.insert(
                    "lnk_target".to_string(),
                    serde_json::json!(link_target.clone()),
                );
                stat.insert("lnk_source".to_string(), serde_json::json!(link_target));
            }

            if params.get_checksum && meta.is_file() && !is_symlink {
                let algorithm = params
                    .checksum_algorithm
                    .as_ref()
                    .unwrap_or(&ChecksumAlgorithm::Sha256);
                match calculate_checksum(path_obj, algorithm) {
                    Ok(checksum) => {
                        stat.insert("checksum".to_string(), serde_json::json!(checksum));
                    }
                    Err(e) => {
                        debug!("Failed to calculate checksum: {}", e);
                        stat.insert("checksum".to_string(), serde_json::json!(null));
                    }
                }
            } else {
                stat.insert("checksum".to_string(), serde_json::json!(null));
            }

            if params.get_md5 && meta.is_file() && !is_symlink {
                match calculate_checksum(path_obj, &ChecksumAlgorithm::Md5) {
                    Ok(md5) => {
                        stat.insert("md5".to_string(), serde_json::json!(md5));
                    }
                    Err(e) => {
                        debug!("Failed to calculate md5: {}", e);
                        stat.insert("md5".to_string(), serde_json::json!(null));
                    }
                }
            } else {
                stat.insert("md5".to_string(), serde_json::json!(null));
            }

            if params.get_mime && meta.is_file() && !is_symlink {
                let mime_type = get_mime_type(path);
                stat.insert("mimetype".to_string(), serde_json::json!(mime_type));
                stat.insert("charset".to_string(), serde_json::json!(null));
            } else {
                stat.insert("mimetype".to_string(), serde_json::json!(null));
                stat.insert("charset".to_string(), serde_json::json!(null));
            }

            if params.get_attributes {
                stat.insert("attributes".to_string(), serde_json::json!([]));
                stat.insert("version".to_string(), serde_json::json!(null));
            } else {
                stat.insert("attributes".to_string(), serde_json::json!(null));
                stat.insert("version".to_string(), serde_json::json!(null));
            }

            stat.insert(
                "pw_name".to_string(),
                serde_json::json!(get_username(meta.uid())),
            );
            stat.insert(
                "gr_name".to_string(),
                serde_json::json!(get_groupname(meta.gid())),
            );
        }
        Err(_) => {
            stat.insert("exists".to_string(), serde_json::json!(false));
            stat.insert("isdir".to_string(), serde_json::json!(false));
            stat.insert("isfile".to_string(), serde_json::json!(false));
            stat.insert("islnk".to_string(), serde_json::json!(false));
            stat.insert("issock".to_string(), serde_json::json!(false));
            stat.insert("isblk".to_string(), serde_json::json!(false));
            stat.insert("ischr".to_string(), serde_json::json!(false));
            stat.insert("isfifo".to_string(), serde_json::json!(false));
            stat.insert("isreg".to_string(), serde_json::json!(false));
            stat.insert("mode".to_string(), serde_json::json!(null));
            stat.insert("readable".to_string(), serde_json::json!(false));
            stat.insert("writeable".to_string(), serde_json::json!(false));
            stat.insert("executable".to_string(), serde_json::json!(false));
            stat.insert("size".to_string(), serde_json::json!(null));
            stat.insert("uid".to_string(), serde_json::json!(null));
            stat.insert("gid".to_string(), serde_json::json!(null));
            stat.insert("atime".to_string(), serde_json::json!(null));
            stat.insert("mtime".to_string(), serde_json::json!(null));
            stat.insert("ctime".to_string(), serde_json::json!(null));
            stat.insert("inode".to_string(), serde_json::json!(null));
            stat.insert("dev".to_string(), serde_json::json!(null));
            stat.insert("nlink".to_string(), serde_json::json!(null));
            stat.insert("blocks".to_string(), serde_json::json!(null));
            stat.insert("blksize".to_string(), serde_json::json!(null));
            stat.insert("lnk_target".to_string(), serde_json::json!(null));
            stat.insert("lnk_source".to_string(), serde_json::json!(null));
            stat.insert("checksum".to_string(), serde_json::json!(null));
            stat.insert("md5".to_string(), serde_json::json!(null));
            stat.insert("mimetype".to_string(), serde_json::json!(null));
            stat.insert("charset".to_string(), serde_json::json!(null));
            stat.insert("attributes".to_string(), serde_json::json!(null));
            stat.insert("version".to_string(), serde_json::json!(null));
            stat.insert("pw_name".to_string(), serde_json::json!(null));
            stat.insert("gr_name".to_string(), serde_json::json!(null));
        }
    }

    Ok(serde_json::Value::Object(stat))
}

fn get_mime_type(path: &str) -> Option<String> {
    use std::process::Command;
    Command::new("file")
        .args(["--mime-type", "-b", path])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn get_username(uid: u32) -> Option<String> {
    unsafe {
        let mut pwd = std::mem::zeroed();
        let mut buf = [0u8; 1024];
        let mut result = std::ptr::null_mut();

        if libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut _,
            buf.len(),
            &mut result,
        ) == 0
            && !result.is_null()
        {
            std::ffi::CStr::from_ptr(pwd.pw_name)
                .to_str()
                .ok()
                .map(String::from)
        } else {
            None
        }
    }
}

fn get_groupname(gid: u32) -> Option<String> {
    unsafe {
        let mut grp = std::mem::zeroed();
        let mut buf = [0u8; 1024];
        let mut result = std::ptr::null_mut();

        if libc::getgrgid_r(
            gid,
            &mut grp,
            buf.as_mut_ptr() as *mut _,
            buf.len(),
            &mut result,
        ) == 0
            && !result.is_null()
        {
            std::ffi::CStr::from_ptr(grp.gr_name)
                .to_str()
                .ok()
                .map(String::from)
        } else {
            None
        }
    }
}

pub fn stat(params: Params) -> Result<ModuleResult> {
    let file_stat = get_file_stat(&params.path, &params)?;
    let extra = value::to_value(json!({"stat": file_stat}))?;

    Ok(ModuleResult {
        changed: false,
        output: None,
        extra: Some(extra),
    })
}

#[derive(Debug)]
pub struct Stat;

impl Module for Stat {
    fn get_name(&self) -> &str {
        "stat"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((stat(parse_params(optional_params)?)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::{create_dir, set_permissions, File};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/app/config.json
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/app/config.json");
        assert_eq!(params.checksum_algorithm, None);
        assert!(!params.follow);
        assert!(params.get_checksum);
        assert!(params.get_md5);
    }

    #[test]
    fn test_parse_params_with_checksum_algorithm() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            checksum_algorithm: sha1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.checksum_algorithm, Some(ChecksumAlgorithm::Sha1));
    }

    #[test]
    fn test_parse_params_with_follow() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            follow: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.follow);
    }

    #[test]
    fn test_stat_file_exists() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_file.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "test content").unwrap();

        let result = stat(Params {
            path: file_path.to_str().unwrap().to_owned(),
            checksum_algorithm: None,
            follow: false,
            get_checksum: true,
            get_md5: true,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        assert!(stat_value["exists"].as_bool().unwrap());
        assert!(stat_value["isfile"].as_bool().unwrap());
        assert!(!stat_value["isdir"].as_bool().unwrap());
        assert!(!stat_value["islnk"].as_bool().unwrap());
        assert!(stat_value["size"].as_u64().unwrap() > 0);
        assert!(stat_value["checksum"].is_string());
    }

    #[test]
    fn test_stat_directory_exists() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().join("test_dir");
        create_dir(&dir_path).unwrap();

        let result = stat(Params {
            path: dir_path.to_str().unwrap().to_owned(),
            checksum_algorithm: None,
            follow: false,
            get_checksum: true,
            get_md5: true,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        assert!(stat_value["exists"].as_bool().unwrap());
        assert!(!stat_value["isfile"].as_bool().unwrap());
        assert!(stat_value["isdir"].as_bool().unwrap());
        assert!(!stat_value["islnk"].as_bool().unwrap());
        assert!(stat_value["checksum"].is_null());
    }

    #[test]
    fn test_stat_file_not_exists() {
        let result = stat(Params {
            path: "/nonexistent/path/to/file".to_owned(),
            checksum_algorithm: None,
            follow: false,
            get_checksum: true,
            get_md5: true,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        assert!(!stat_value["exists"].as_bool().unwrap());
        assert!(!stat_value["isfile"].as_bool().unwrap());
        assert!(!stat_value["isdir"].as_bool().unwrap());
    }

    #[test]
    fn test_stat_symlink() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("original.txt");
        let link_path = dir.path().join("link.txt");

        File::create(&file_path).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&file_path, &link_path).unwrap();

        let result = stat(Params {
            path: link_path.to_str().unwrap().to_owned(),
            checksum_algorithm: None,
            follow: false,
            get_checksum: true,
            get_md5: true,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        assert!(stat_value["exists"].as_bool().unwrap());
        assert!(stat_value["islnk"].as_bool().unwrap());
        assert!(stat_value["lnk_target"].is_string());
    }

    #[test]
    fn test_stat_checksum_sha256() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "hello").unwrap();

        let result = stat(Params {
            path: file_path.to_str().unwrap().to_owned(),
            checksum_algorithm: Some(ChecksumAlgorithm::Sha256),
            follow: false,
            get_checksum: true,
            get_md5: false,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        let checksum = stat_value["checksum"].as_str().unwrap();
        assert_eq!(
            checksum,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_stat_checksum_sha1() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "hello").unwrap();

        let result = stat(Params {
            path: file_path.to_str().unwrap().to_owned(),
            checksum_algorithm: Some(ChecksumAlgorithm::Sha1),
            follow: false,
            get_checksum: true,
            get_md5: false,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        let checksum = stat_value["checksum"].as_str().unwrap();
        assert_eq!(checksum, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    }

    #[test]
    fn test_stat_checksum_md5() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "hello").unwrap();

        let result = stat(Params {
            path: file_path.to_str().unwrap().to_owned(),
            checksum_algorithm: Some(ChecksumAlgorithm::Md5),
            follow: false,
            get_checksum: true,
            get_md5: true,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        let checksum = stat_value["checksum"].as_str().unwrap();
        assert_eq!(checksum, "5d41402abc4b2a76b9719d911017c592");
        let md5 = stat_value["md5"].as_str().unwrap();
        assert_eq!(md5, "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_stat_permissions() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let file = File::create(&file_path).unwrap();
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        set_permissions(&file_path, perms).unwrap();

        let result = stat(Params {
            path: file_path.to_str().unwrap().to_owned(),
            checksum_algorithm: None,
            follow: false,
            get_checksum: false,
            get_md5: false,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        assert!(stat_value["readable"].as_bool().unwrap());
        assert!(stat_value["writeable"].as_bool().unwrap());
        assert!(stat_value["executable"].as_bool().unwrap());
        assert_eq!(stat_value["mode"].as_str().unwrap(), "0755");
    }

    #[test]
    fn test_stat_no_checksum() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = stat(Params {
            path: file_path.to_str().unwrap().to_owned(),
            checksum_algorithm: None,
            follow: false,
            get_checksum: false,
            get_md5: false,
            get_mime: false,
            get_attributes: true,
        })
        .unwrap();

        let extra = result.extra.unwrap();
        let stat_value = extra.get("stat").unwrap();
        assert!(stat_value["checksum"].is_null());
    }
}
