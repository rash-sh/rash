/// ANCHOR: module
/// # iso_extract
///
/// Extract contents from ISO files.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// diff_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Extract entire ISO
///   iso_extract:
///     iso: /tmp/image.iso
///     dest: /mnt/extracted
///
/// - name: Extract specific files from ISO
///   iso_extract:
///     iso: /tmp/install.iso
///     dest: /mnt/packages
///     files:
///       - /packages/core.pkg
///       - /packages/utils.pkg
///
/// - name: Extract ISO with overwrite
///   iso_extract:
///     iso: /tmp/image.iso
///     dest: /mnt/extracted
///     force: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{File, create_dir_all};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use iso9660::{DirectoryEntry, ISO9660};
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the ISO file to extract.
    pub iso: String,
    /// Destination directory where files will be extracted.
    pub dest: String,
    /// List of specific files to extract from the ISO.
    /// If not specified, all files will be extracted.
    pub files: Option<Vec<String>>,
    /// Overwrite existing files in the destination directory.
    /// **[default: `false`]**
    #[serde(default)]
    pub force: bool,
}

fn extract_directory<T: iso9660::ISO9660Reader>(
    dir: &iso9660::ISODirectory<T>,
    dest: &Path,
    force: bool,
) -> Result<(u64, usize)> {
    let mut total_size = 0u64;
    let mut files_extracted = 0usize;

    for entry_result in dir.contents() {
        let entry = entry_result.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read directory entry: {:?}", e),
            )
        })?;

        match entry {
            DirectoryEntry::Directory(d) => {
                if d.identifier == "." || d.identifier == ".." {
                    continue;
                }
                let child_dest = dest.join(&d.identifier);
                create_dir_all(&child_dest).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to create directory {}: {e}", child_dest.display()),
                    )
                })?;
                let (size, count) = extract_directory(&d, &child_dest, force)?;
                total_size += size;
                files_extracted += count;
            }
            DirectoryEntry::File(f) => {
                let child_dest = dest.join(&f.identifier);
                if child_dest.exists() && !force {
                    trace!("File {} already exists, skipping", child_dest.display());
                    continue;
                }

                let mut reader = f.read();
                let mut buffer = Vec::new();
                reader.read_to_end(&mut buffer).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to read file from ISO: {e}"),
                    )
                })?;

                let mut outfile = File::create(&child_dest).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to create file {}: {e}", child_dest.display()),
                    )
                })?;

                outfile.write_all(&buffer).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to write file {}: {e}", child_dest.display()),
                    )
                })?;

                total_size += buffer.len() as u64;
                files_extracted += 1;
            }
        }
    }

    Ok((total_size, files_extracted))
}

fn extract_single_file<T: iso9660::ISO9660Reader>(
    entry: &DirectoryEntry<T>,
    dest: &Path,
    force: bool,
) -> Result<(u64, usize)> {
    match entry {
        DirectoryEntry::File(f) => {
            let file_dest = dest.join(&f.identifier);
            if file_dest.exists() && !force {
                trace!("File {} already exists, skipping", file_dest.display());
                return Ok((0, 0));
            }

            if let Some(parent) = file_dest.parent() {
                create_dir_all(parent).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "Failed to create parent directory {}: {e}",
                            parent.display()
                        ),
                    )
                })?;
            }

            let mut reader = f.read();
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to read file from ISO: {e}"),
                )
            })?;

            let mut outfile = File::create(&file_dest).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to create file {}: {e}", file_dest.display()),
                )
            })?;

            outfile.write_all(&buffer).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to write file {}: {e}", file_dest.display()),
                )
            })?;

            Ok((buffer.len() as u64, 1))
        }
        DirectoryEntry::Directory(d) => {
            let dir_dest = dest.join(&d.identifier);
            create_dir_all(&dir_dest).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to create directory {}: {e}", dir_dest.display()),
                )
            })?;
            extract_directory(d, &dir_dest, force)
        }
    }
}

fn run_iso_extract(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let iso_path = PathBuf::from(&params.iso);
    let dest_path = PathBuf::from(&params.dest);

    if check_mode {
        let file_count = params.files.as_ref().map(|f| f.len()).unwrap_or(0);
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would extract {} from {} to {}",
                if file_count > 0 {
                    format!("{} files", file_count)
                } else {
                    "all files".to_string()
                },
                iso_path.display(),
                dest_path.display()
            )),
            extra: None,
        });
    }

    if !iso_path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("ISO file not found: {}", iso_path.display()),
        ));
    }

    let file = File::open(&iso_path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to open ISO file {}: {e}", iso_path.display()),
        )
    })?;

    let iso = ISO9660::new(file).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse ISO file {}: {:?}", e, iso_path.display()),
        )
    })?;

    create_dir_all(&dest_path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "Failed to create destination directory {}: {e}",
                dest_path.display()
            ),
        )
    })?;

    diff(
        "",
        format!(
            "Extracting ISO {} to {}\n",
            iso_path.display(),
            dest_path.display()
        ),
    );

    let mut total_size = 0u64;
    let mut files_extracted = 0usize;

    if let Some(files) = &params.files {
        for file_path in files {
            let entry = iso.open(file_path).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to open path {} in ISO: {:?}", file_path, e),
                )
            })?;

            if let Some(entry) = entry {
                let (size, count) = extract_single_file(&entry, &dest_path, params.force)?;
                total_size += size;
                files_extracted += count;
            } else {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("File not found in ISO: {}", file_path),
                ));
            }
        }
    } else {
        let (size, count) = extract_directory(&iso.root, &dest_path, params.force)?;
        total_size = size;
        files_extracted = count;
    }

    diff(
        "",
        format!(
            "Extracted {} files ({} bytes total)\n",
            files_extracted, total_size
        ),
    );

    Ok(ModuleResult {
        changed: files_extracted > 0,
        output: Some(format!(
            "Extracted {} files ({}) from {} to {}",
            files_extracted,
            format_bytes(total_size),
            iso_path.display(),
            dest_path.display()
        )),
        extra: Some(serde_norway::Value::Mapping(
            serde_norway::Mapping::from_iter([
                (
                    serde_norway::Value::String("files_extracted".to_string()),
                    serde_norway::Value::Number(files_extracted.into()),
                ),
                (
                    serde_norway::Value::String("size".to_string()),
                    serde_norway::Value::Number(total_size.into()),
                ),
            ]),
        )),
    })
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Debug)]
pub struct IsoExtract;

impl Module for IsoExtract {
    fn get_name(&self) -> &str {
        "iso_extract"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((run_iso_extract(parse_params(params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_basic() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            iso: /tmp/image.iso
            dest: /mnt/extracted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.iso, "/tmp/image.iso");
        assert_eq!(params.dest, "/mnt/extracted");
        assert!(params.files.is_none());
        assert!(!params.force);
    }

    #[test]
    fn test_parse_params_with_files() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            iso: /tmp/install.iso
            dest: /mnt/packages
            files:
              - /packages/core.pkg
              - /packages/utils.pkg
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.iso, "/tmp/install.iso");
        assert_eq!(
            params.files,
            Some(vec![
                "/packages/core.pkg".to_string(),
                "/packages/utils.pkg".to_string()
            ])
        );
        assert!(params.force);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_run_iso_extract_missing_iso() {
        let params = Params {
            iso: "/nonexistent/file.iso".to_string(),
            dest: "/tmp/dest".to_string(),
            files: None,
            force: false,
        };

        let result = run_iso_extract(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_iso_extract_check_mode() {
        let params = Params {
            iso: "/tmp/image.iso".to_string(),
            dest: "/mnt/extracted".to_string(),
            files: Some(vec!["/file.txt".to_string()]),
            force: false,
        };

        let result = run_iso_extract(params, true).unwrap();
        assert!(result.changed);
        assert!(result.output.unwrap().contains("Would extract"));
    }
}
