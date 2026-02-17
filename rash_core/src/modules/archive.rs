/// ANCHOR: module
/// # archive
///
/// Creates a compressed archive of one or more files or directories.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: partial
/// diff_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - archive:
///     path: /var/log/app
///     dest: /backup/logs.tar.gz
///
/// - archive:
///     path:
///       - /etc/nginx
///       - /etc/apache2
///     dest: /backup/web-configs.tar.bz2
///     format: bz2
///
/// - archive:
///     path: /home/user/data
///     dest: /backup/data.tar.xz
///     format: xz
///     exclude:
///       - "*.tmp"
///       - "*.cache"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use tar::Builder as TarBuilder;

#[derive(Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Gz,
    Bz2,
    Xz,
    Tar,
    Zip,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Gz => write!(f, "gz"),
            Format::Bz2 => write!(f, "bz2"),
            Format::Xz => write!(f, "xz"),
            Format::Tar => write!(f, "tar"),
            Format::Zip => write!(f, "zip"),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Remote absolute path, list of paths, or glob patterns for the file or files to archive.
    #[serde(deserialize_with = "deserialize_path")]
    pub path: Vec<String>,
    /// The file name of the destination archive.
    pub dest: String,
    /// The type of compression to use.
    /// **[default: `"gz"`]**
    #[serde(default)]
    pub format: Format,
    /// List of patterns to exclude from the archive.
    pub exclude: Option<Vec<String>>,
    /// Remove the original file tree after archiving.
    #[serde(default)]
    pub remove: bool,
    /// Force archiving even if the destination archive already exists.
    #[serde(default)]
    pub force: bool,
}

fn deserialize_path<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let value: serde_norway::Value = serde::Deserialize::deserialize(deserializer)?;

    match value {
        serde_norway::Value::String(s) => Ok(vec![s]),
        serde_norway::Value::Sequence(seq) => seq
            .into_iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| D::Error::custom("path elements must be strings"))
            })
            .collect(),
        _ => Err(D::Error::custom(
            "path must be a string or array of strings",
        )),
    }
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    if let (Some(middle_start), Some(middle_end)) =
        (pattern.strip_prefix('*'), pattern.strip_suffix('*'))
        && middle_start == middle_end
    {
        let middle = middle_start;
        name.contains(middle)
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            name.starts_with(parts[0]) && name.ends_with(parts[1])
        } else {
            name == pattern
        }
    } else {
        name == pattern || name.ends_with(&format!("/{pattern}"))
    }
}

fn should_exclude(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if matches_pattern(path, pattern) {
            return true;
        }
        let path_name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if matches_pattern(path_name, pattern) {
            return true;
        }
    }
    false
}

fn expand_paths(paths: &[String]) -> Result<Vec<PathBuf>> {
    let mut expanded = Vec::new();

    for path_str in paths {
        let path = PathBuf::from(path_str);

        if path_str.contains('*')
            || path_str.contains('?')
            || path_str.contains('[') && path_str.contains(']')
        {
            let pattern = path_str;
            for entry in glob::glob(pattern).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid glob pattern '{}': {e}", pattern),
                )
            })? {
                let entry = entry
                    .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Glob error: {e}")))?;
                expanded.push(entry);
            }
        } else if path.exists() {
            expanded.push(path);
        } else {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Path not found: {}", path.display()),
            ));
        }
    }

    if expanded.is_empty() {
        return Err(Error::new(
            ErrorKind::NotFound,
            "No files or directories found to archive",
        ));
    }

    Ok(expanded)
}

fn add_path_to_tar<W: std::io::Write>(
    tar: &mut TarBuilder<W>,
    path: &Path,
    base_path: &Path,
    exclude: &[String],
) -> Result<u64> {
    let mut count = 0;

    if path.is_file() {
        let relative = path.strip_prefix(base_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to strip path prefix: {e}"),
            )
        })?;

        let relative_str = relative.to_string_lossy();
        if should_exclude(&relative_str, exclude) {
            trace!("Excluding: {}", relative_str);
            return Ok(0);
        }

        tar.append_path_with_name(path, relative).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to add {} to archive: {e}", path.display()),
            )
        })?;

        return Ok(1);
    }

    if path.is_dir() {
        for entry in walkdir::WalkDir::new(path).follow_links(false) {
            let entry = entry.map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to walk directory: {e}"),
                )
            })?;

            let entry_path = entry.path();

            let relative = entry_path.strip_prefix(base_path).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to strip path prefix: {e}"),
                )
            })?;

            let relative_str = relative.to_string_lossy();

            if should_exclude(&relative_str, exclude) {
                trace!("Excluding: {}", relative_str);
                continue;
            }

            if entry_path.is_dir() {
                tar.append_dir(relative, entry_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "Failed to add directory {} to archive: {e}",
                            entry_path.display()
                        ),
                    )
                })?;
            } else {
                tar.append_path_with_name(entry_path, relative)
                    .map_err(|e| {
                        Error::new(
                            ErrorKind::InvalidData,
                            format!(
                                "Failed to add file {} to archive: {e}",
                                entry_path.display()
                            ),
                        )
                    })?;
            }

            count += 1;
        }
    }

    Ok(count)
}

fn create_tar_gz(paths: &[PathBuf], dest: &Path, exclude: &[String]) -> Result<u64> {
    let file = File::create(dest).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create archive {}: {e}", dest.display()),
        )
    })?;

    let encoder = GzEncoder::new(file, flate2::Compression::default());
    let mut tar = TarBuilder::new(encoder);

    let mut total_count = 0;
    for path in paths {
        let base = if path.is_dir() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let count = add_path_to_tar(&mut tar, path, base, exclude)?;
        total_count += count;
    }

    tar.finish().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to finalize archive: {e}"),
        )
    })?;

    Ok(total_count)
}

fn create_tar_bz2(paths: &[PathBuf], dest: &Path, exclude: &[String]) -> Result<u64> {
    let file = File::create(dest).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create archive {}: {e}", dest.display()),
        )
    })?;

    let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());
    let mut tar = TarBuilder::new(encoder);

    let mut total_count = 0;
    for path in paths {
        let base = if path.is_dir() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let count = add_path_to_tar(&mut tar, path, base, exclude)?;
        total_count += count;
    }

    tar.finish().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to finalize archive: {e}"),
        )
    })?;

    Ok(total_count)
}

fn create_tar_xz(paths: &[PathBuf], dest: &Path, exclude: &[String]) -> Result<u64> {
    let file = File::create(dest).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create archive {}: {e}", dest.display()),
        )
    })?;

    let encoder = xz2::write::XzEncoder::new(file, 6);
    let mut tar = TarBuilder::new(encoder);

    let mut total_count = 0;
    for path in paths {
        let base = if path.is_dir() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let count = add_path_to_tar(&mut tar, path, base, exclude)?;
        total_count += count;
    }

    tar.finish().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to finalize archive: {e}"),
        )
    })?;

    Ok(total_count)
}

fn create_tar(paths: &[PathBuf], dest: &Path, exclude: &[String]) -> Result<u64> {
    let file = File::create(dest).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create archive {}: {e}", dest.display()),
        )
    })?;

    let mut tar = TarBuilder::new(file);

    let mut total_count = 0;
    for path in paths {
        let base = if path.is_dir() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let count = add_path_to_tar(&mut tar, path, base, exclude)?;
        total_count += count;
    }

    tar.finish().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to finalize archive: {e}"),
        )
    })?;

    Ok(total_count)
}

fn create_zip(paths: &[PathBuf], dest: &Path, exclude: &[String]) -> Result<u64> {
    let file = File::create(dest).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create archive {}: {e}", dest.display()),
        )
    })?;

    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut total_count = 0;

    for path in paths {
        let base = if path.is_dir() {
            path.parent().unwrap_or(path)
        } else {
            path
        };

        for entry in walkdir::WalkDir::new(path).follow_links(false) {
            let entry = entry.map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to walk directory: {e}"),
                )
            })?;

            let entry_path = entry.path();

            let relative = entry_path.strip_prefix(base).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to strip path prefix: {e}"),
                )
            })?;

            let relative_str = relative.to_string_lossy();

            if should_exclude(&relative_str, exclude) {
                trace!("Excluding: {}", relative_str);
                continue;
            }

            if entry_path.is_dir() {
                let dir_name = format!("{}/", relative_str);
                zip.add_directory(&dir_name, options).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to add directory to zip: {e}"),
                    )
                })?;
            } else {
                zip.start_file(relative_str.to_string(), options)
                    .map_err(|e| {
                        Error::new(
                            ErrorKind::InvalidData,
                            format!("Failed to add file to zip: {e}"),
                        )
                    })?;

                let mut file = File::open(entry_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to open {}: {e}", entry_path.display()),
                    )
                })?;

                std::io::copy(&mut file, &mut zip).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to write file to zip: {e}"),
                    )
                })?;
            }

            total_count += 1;
        }
    }

    zip.finish().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to finalize zip archive: {e}"),
        )
    })?;

    Ok(total_count)
}

fn remove_paths(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to remove directory {}: {e}", path.display()),
                )
            })?;
        } else {
            fs::remove_file(path).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to remove file {}: {e}", path.display()),
                )
            })?;
        }
    }
    Ok(())
}

fn run_archive(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let dest = PathBuf::from(&params.dest);

    if dest.exists() && !params.force {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!(
                "Archive {} already exists. Use force=true to overwrite.",
                dest.display()
            )),
            extra: None,
        });
    }

    let expanded_paths = expand_paths(&params.path)?;

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would create archive {} from {} paths",
                dest.display(),
                expanded_paths.len()
            )),
            extra: None,
        });
    }

    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Failed to create parent directory {}: {e}",
                    parent.display()
                ),
            )
        })?;
    }

    let exclude = params.exclude.as_deref().unwrap_or(&[]);

    diff(
        "",
        format!(
            "Creating archive {} from {} paths\n",
            dest.display(),
            expanded_paths.len()
        ),
    );

    let count = match &params.format {
        Format::Gz => create_tar_gz(&expanded_paths, &dest, exclude)?,
        Format::Bz2 => create_tar_bz2(&expanded_paths, &dest, exclude)?,
        Format::Xz => create_tar_xz(&expanded_paths, &dest, exclude)?,
        Format::Tar => create_tar(&expanded_paths, &dest, exclude)?,
        Format::Zip => create_zip(&expanded_paths, &dest, exclude)?,
    };

    if params.remove {
        diff(
            "",
            format!("Removing {} source paths\n", expanded_paths.len()),
        );
        remove_paths(&expanded_paths)?;
    }

    let archive_size = dest.metadata().map(|m| m.len()).unwrap_or(0);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Created archive {} ({} entries, {} bytes)",
            dest.display(),
            count,
            archive_size
        )),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Archive;

impl Module for Archive {
    fn get_name(&self) -> &str {
        "archive"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((run_archive(parse_params(params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::{self, File};
    use std::io::Write;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params_single_path() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /var/log/app
            dest: /backup/logs.tar.gz
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, vec!["/var/log/app"]);
        assert_eq!(params.dest, "/backup/logs.tar.gz");
        assert_eq!(params.format, Format::Gz);
    }

    #[test]
    fn test_parse_params_multiple_paths() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path:
              - /etc/nginx
              - /etc/apache2
            dest: /backup/web-configs.tar.bz2
            format: bz2
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, vec!["/etc/nginx", "/etc/apache2"]);
        assert_eq!(params.format, Format::Bz2);
    }

    #[test]
    fn test_parse_params_with_exclude() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /home/user/data
            dest: /backup/data.tar.xz
            format: xz
            exclude:
              - "*.tmp"
              - "*.cache"
            remove: true
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.format, Format::Xz);
        assert_eq!(
            params.exclude,
            Some(vec!["*.tmp".to_string(), "*.cache".to_string()])
        );
        assert!(params.remove);
        assert!(params.force);
    }

    #[test]
    fn test_format_display() {
        assert_eq!(format!("{}", Format::Gz), "gz");
        assert_eq!(format!("{}", Format::Bz2), "bz2");
        assert_eq!(format!("{}", Format::Xz), "xz");
        assert_eq!(format!("{}", Format::Tar), "tar");
        assert_eq!(format!("{}", Format::Zip), "zip");
    }

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("test.log", "*.log"));
        assert!(matches_pattern("file.tmp", "*.tmp"));
        assert!(matches_pattern("dir/test.log", "*.log"));
        assert!(matches_pattern("test", "test"));
        assert!(!matches_pattern("test.txt", "*.log"));
    }

    #[test]
    fn test_should_exclude() {
        let patterns = vec!["*.log".to_string(), "*.tmp".to_string()];
        assert!(should_exclude("test.log", &patterns));
        assert!(should_exclude("dir/test.log", &patterns));
        assert!(should_exclude("file.tmp", &patterns));
        assert!(!should_exclude("file.txt", &patterns));
    }

    #[test]
    fn test_expand_paths_single_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let expanded = expand_paths(&[file_path.to_str().unwrap().to_string()]).unwrap();

        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0], file_path);
    }

    #[test]
    fn test_expand_paths_directory() {
        let dir = tempdir().unwrap();
        let sub_dir = dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        File::create(sub_dir.join("file.txt")).unwrap();

        let expanded = expand_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0], dir.path());
    }

    #[test]
    fn test_expand_paths_missing() {
        let result = expand_paths(&["/nonexistent/path".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_tar_gz() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file1 = src_dir.join("file1.txt");
        let mut f1 = File::create(&file1).unwrap();
        writeln!(f1, "content1").unwrap();

        let file2 = src_dir.join("file2.txt");
        let mut f2 = File::create(&file2).unwrap();
        writeln!(f2, "content2").unwrap();

        let archive_path = dir.path().join("test.tar.gz");

        let count = create_tar_gz(std::slice::from_ref(&src_dir), &archive_path, &[]).unwrap();

        assert!(archive_path.exists());
        assert!(count >= 2);

        let dest_dir = dir.path().join("extracted");
        fs::create_dir(&dest_dir).unwrap();

        let file = File::open(&archive_path).unwrap();
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&dest_dir).unwrap();

        assert!(dest_dir.join("src/file1.txt").exists());
        assert!(dest_dir.join("src/file2.txt").exists());
    }

    #[test]
    fn test_create_tar_gz_with_exclude() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file1 = src_dir.join("file.txt");
        let mut f1 = File::create(&file1).unwrap();
        writeln!(f1, "content").unwrap();

        let file2 = src_dir.join("file.log");
        let mut f2 = File::create(&file2).unwrap();
        writeln!(f2, "log content").unwrap();

        let archive_path = dir.path().join("test.tar.gz");
        let exclude = vec!["*.log".to_string()];

        let _count =
            create_tar_gz(std::slice::from_ref(&src_dir), &archive_path, &exclude).unwrap();

        let dest_dir = dir.path().join("extracted");
        fs::create_dir(&dest_dir).unwrap();

        let file = File::open(&archive_path).unwrap();
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&dest_dir).unwrap();

        assert!(dest_dir.join("src/file.txt").exists());
        assert!(!dest_dir.join("src/file.log").exists());
    }

    #[test]
    fn test_create_zip() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file1 = src_dir.join("file1.txt");
        let mut f1 = File::create(&file1).unwrap();
        writeln!(f1, "content1").unwrap();

        let archive_path = dir.path().join("test.zip");

        let count = create_zip(std::slice::from_ref(&src_dir), &archive_path, &[]).unwrap();

        assert!(archive_path.exists());
        assert!(count >= 1);

        let file = File::open(&archive_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        assert!(archive.by_name("src/file1.txt").is_ok());
    }

    #[test]
    fn test_run_archive_creates_archive() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file = src_dir.join("test.txt");
        let mut f = File::create(&file).unwrap();
        writeln!(f, "test content").unwrap();

        let archive_path = dir.path().join("archive.tar.gz");

        let params = Params {
            path: vec![src_dir.to_str().unwrap().to_string()],
            dest: archive_path.to_str().unwrap().to_string(),
            format: Format::Gz,
            exclude: None,
            remove: false,
            force: false,
        };

        let result = run_archive(params, false).unwrap();

        assert!(result.changed);
        assert!(archive_path.exists());
    }

    #[test]
    fn test_run_archive_check_mode() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file = src_dir.join("test.txt");
        File::create(&file).unwrap();

        let archive_path = dir.path().join("archive.tar.gz");

        let params = Params {
            path: vec![src_dir.to_str().unwrap().to_string()],
            dest: archive_path.to_str().unwrap().to_string(),
            format: Format::Gz,
            exclude: None,
            remove: false,
            force: false,
        };

        let result = run_archive(params, true).unwrap();

        assert!(result.changed);
        assert!(!archive_path.exists());
    }

    #[test]
    fn test_run_archive_existing_no_force() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file = src_dir.join("test.txt");
        File::create(&file).unwrap();

        let archive_path = dir.path().join("archive.tar.gz");
        File::create(&archive_path).unwrap();

        let params = Params {
            path: vec![src_dir.to_str().unwrap().to_string()],
            dest: archive_path.to_str().unwrap().to_string(),
            format: Format::Gz,
            exclude: None,
            remove: false,
            force: false,
        };

        let result = run_archive(params, false).unwrap();

        assert!(!result.changed);
    }

    #[test]
    fn test_run_archive_existing_with_force() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file = src_dir.join("test.txt");
        File::create(&file).unwrap();

        let archive_path = dir.path().join("archive.tar.gz");
        File::create(&archive_path).unwrap();

        let initial_size = archive_path.metadata().unwrap().len();

        let params = Params {
            path: vec![src_dir.to_str().unwrap().to_string()],
            dest: archive_path.to_str().unwrap().to_string(),
            format: Format::Gz,
            exclude: None,
            remove: false,
            force: true,
        };

        let result = run_archive(params, false).unwrap();

        assert!(result.changed);

        let new_size = archive_path.metadata().unwrap().len();
        assert!(new_size > initial_size);
    }

    #[test]
    fn test_run_archive_with_remove() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        let file = src_dir.join("test.txt");
        let mut f = File::create(&file).unwrap();
        writeln!(f, "test content").unwrap();

        let archive_path = dir.path().join("archive.tar.gz");

        let params = Params {
            path: vec![src_dir.to_str().unwrap().to_string()],
            dest: archive_path.to_str().unwrap().to_string(),
            format: Format::Gz,
            exclude: None,
            remove: true,
            force: false,
        };

        let result = run_archive(params, false).unwrap();

        assert!(result.changed);
        assert!(archive_path.exists());
        assert!(!src_dir.exists());
    }
}
