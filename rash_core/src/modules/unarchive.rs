/// ANCHOR: module
/// # unarchive
///
/// Unpacks an archive (tar, tar.gz, tar.bz2, tar.xz, zip) to a destination.
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
/// - unarchive:
///     src: /tmp/app.tar.gz
///     dest: /opt/app
///
/// - unarchive:
///     src: https://example.com/package.tar.gz
///     dest: /opt/package
///     remote_src: yes
///
/// - unarchive:
///     src: /tmp/backup.tar.gz
///     dest: /var/app
///     exclude:
///       - "*.log"
///       - "*.tmp"
///     mode: "0755"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashSet;
use std::fs::{self, File, create_dir_all};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use tar::Archive as TarArchive;
use zip::ZipArchive;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the archive file to unpack.
    /// If remote_src is true, this can be a URL.
    pub src: String,
    /// Remote absolute path where the archive should be unpacked.
    pub dest: String,
    /// If true, src is a URL and will be downloaded first.
    #[serde(default)]
    pub remote_src: bool,
    /// List of directory and file patterns to exclude from extraction.
    pub exclude: Option<Vec<String>>,
    /// The permissions the extracted files and directories should have.
    pub mode: Option<String>,
    /// Name of the group that should own the extracted files.
    pub group: Option<String>,
    /// Name of the user that should own the extracted files.
    pub owner: Option<String>,
    /// If true, the destination directory will be created if it does not exist.
    #[serde(default = "default_create_dest")]
    pub create_dest: bool,
    /// Checksum of the archive file (format: algorithm:hash).
    pub checksum: Option<String>,
}

fn default_create_dest() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ArchiveFormat {
    TarGz,
    TarBz2,
    TarXz,
    Tar,
    Zip,
}

impl ArchiveFormat {
    fn detect_from_path(path: &Path) -> Option<Self> {
        let path_str = path.to_string_lossy().to_lowercase();
        if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
            Some(Self::TarGz)
        } else if path_str.ends_with(".tar.bz2") || path_str.ends_with(".tbz2") {
            Some(Self::TarBz2)
        } else if path_str.ends_with(".tar.xz") || path_str.ends_with(".txz") {
            Some(Self::TarXz)
        } else if path_str.ends_with(".zip") {
            Some(Self::Zip)
        } else if path_str.ends_with(".tar") {
            Some(Self::Tar)
        } else {
            None
        }
    }

    fn detect_from_content<R: Read + Seek>(reader: &mut R) -> Result<Option<Self>> {
        let mut magic = [0u8; 6];
        let bytes_read = reader.read(&mut magic).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read magic bytes: {e}"),
            )
        })?;
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Failed to seek: {e}")))?;

        if bytes_read < 2 {
            return Ok(None);
        }

        if magic[0] == 0x1f && magic[1] == 0x8b {
            return Ok(Some(Self::TarGz));
        }

        if bytes_read >= 3 && magic[0] == b'B' && magic[1] == b'Z' && magic[2] == b'h' {
            return Ok(Some(Self::TarBz2));
        }

        if bytes_read >= 6
            && magic[0] == 0xfd
            && magic[1] == b'7'
            && magic[2] == b'z'
            && magic[3] == b'X'
            && magic[4] == b'Z'
            && magic[5] == 0x00
        {
            return Ok(Some(Self::TarXz));
        }

        if bytes_read >= 4
            && magic[0] == 0x50
            && magic[1] == 0x4b
            && magic[2] == 0x03
            && magic[3] == 0x04
        {
            return Ok(Some(Self::Zip));
        }

        Ok(None)
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

fn set_permissions_recursively(path: &Path, mode: u32) -> Result<()> {
    let permissions = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to set permissions on {}: {e}", path.display()),
        )
    })?;

    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read directory {}: {e}", path.display()),
            )
        })? {
            let entry = entry.map_err(|e| {
                Error::new(ErrorKind::InvalidData, format!("Failed to read entry: {e}"))
            })?;
            set_permissions_recursively(&entry.path(), mode)?;
        }
    }

    Ok(())
}

fn download_remote_file(url: &str, dest: &Path) -> Result<()> {
    use std::io::Write;

    let response = reqwest::blocking::get(url).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to download from {}: {e}", url),
        )
    })?;

    if !response.status().is_success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("HTTP request failed with status: {}", response.status()),
        ));
    }

    let mut file = File::create(dest).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create file {}: {e}", dest.display()),
        )
    })?;

    let content = response.bytes().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to read response body: {e}"),
        )
    })?;

    file.write_all(&content).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to write to file {}: {e}", dest.display()),
        )
    })?;

    Ok(())
}

fn calculate_checksum(path: &Path, algorithm: &str) -> Result<String> {
    let contents = fs::read(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read file for checksum: {e}"),
        )
    })?;

    match algorithm.to_lowercase().as_str() {
        "sha256" => {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&contents);
            Ok(format!("{:x}", hasher.finalize()))
        }
        "md5" => {
            use md5::{Digest, Md5};
            let mut hasher = Md5::new();
            hasher.update(&contents);
            Ok(format!("{:x}", hasher.finalize()))
        }
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Unsupported checksum algorithm: {algorithm}"),
        )),
    }
}

fn parse_checksum(checksum: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = checksum.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Checksum must be in format 'algorithm:hash'".to_string(),
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn extract_tar_gz<R: Read>(reader: R, dest: &Path, exclude: &[String]) -> Result<HashSet<PathBuf>> {
    let decoder = GzDecoder::new(reader);
    let mut archive = TarArchive::new(decoder);
    let mut extracted = HashSet::new();

    for entry in archive.entries().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read tar entries: {e}"),
        )
    })? {
        let mut entry = entry.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read tar entry: {e}"),
            )
        })?;

        let path = entry.path().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid path in archive: {e}"),
            )
        })?;

        let path_str = path.to_string_lossy();

        if should_exclude(&path_str, exclude) {
            trace!("Excluding: {}", path_str);
            continue;
        }

        let dest_path = dest.join(path);

        entry.unpack(&dest_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to extract {}: {e}", dest_path.display()),
            )
        })?;

        extracted.insert(dest_path);
    }

    Ok(extracted)
}

fn extract_tar_bz2<R: Read>(
    reader: R,
    dest: &Path,
    exclude: &[String],
) -> Result<HashSet<PathBuf>> {
    let decoder = bzip2::read::BzDecoder::new(reader);
    let mut archive = TarArchive::new(decoder);
    let mut extracted = HashSet::new();

    for entry in archive.entries().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read tar entries: {e}"),
        )
    })? {
        let mut entry = entry.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read tar entry: {e}"),
            )
        })?;

        let path = entry.path().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid path in archive: {e}"),
            )
        })?;

        let path_str = path.to_string_lossy();

        if should_exclude(&path_str, exclude) {
            trace!("Excluding: {}", path_str);
            continue;
        }

        let dest_path = dest.join(path);

        entry.unpack(&dest_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to extract {}: {e}", dest_path.display()),
            )
        })?;

        extracted.insert(dest_path);
    }

    Ok(extracted)
}

fn extract_tar_xz<R: Read>(reader: R, dest: &Path, exclude: &[String]) -> Result<HashSet<PathBuf>> {
    let decoder = xz2::read::XzDecoder::new(reader);
    let mut archive = TarArchive::new(decoder);
    let mut extracted = HashSet::new();

    for entry in archive.entries().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read tar entries: {e}"),
        )
    })? {
        let mut entry = entry.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read tar entry: {e}"),
            )
        })?;

        let path = entry.path().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid path in archive: {e}"),
            )
        })?;

        let path_str = path.to_string_lossy();

        if should_exclude(&path_str, exclude) {
            trace!("Excluding: {}", path_str);
            continue;
        }

        let dest_path = dest.join(path);

        entry.unpack(&dest_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to extract {}: {e}", dest_path.display()),
            )
        })?;

        extracted.insert(dest_path);
    }

    Ok(extracted)
}

fn extract_tar<R: Read>(reader: R, dest: &Path, exclude: &[String]) -> Result<HashSet<PathBuf>> {
    let mut archive = TarArchive::new(reader);
    let mut extracted = HashSet::new();

    for entry in archive.entries().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read tar entries: {e}"),
        )
    })? {
        let mut entry = entry.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read tar entry: {e}"),
            )
        })?;

        let path = entry.path().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid path in archive: {e}"),
            )
        })?;

        let path_str = path.to_string_lossy();

        if should_exclude(&path_str, exclude) {
            trace!("Excluding: {}", path_str);
            continue;
        }

        let dest_path = dest.join(path);

        entry.unpack(&dest_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to extract {}: {e}", dest_path.display()),
            )
        })?;

        extracted.insert(dest_path);
    }

    Ok(extracted)
}

fn extract_zip<R: Read + Seek>(
    reader: R,
    dest: &Path,
    exclude: &[String],
) -> Result<HashSet<PathBuf>> {
    let mut archive = ZipArchive::new(reader).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read zip archive: {e}"),
        )
    })?;

    let mut extracted = HashSet::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read zip entry {}: {e}", i),
            )
        })?;

        let path_str = file.name().to_string();

        if should_exclude(&path_str, exclude) {
            trace!("Excluding: {}", path_str);
            continue;
        }

        let dest_path = dest.join(&path_str);

        if file.is_dir() {
            create_dir_all(&dest_path).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to create directory {}: {e}", dest_path.display()),
                )
            })?;
        } else {
            if let Some(parent) = dest_path.parent() {
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

            let mut outfile = File::create(&dest_path).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to create file {}: {e}", dest_path.display()),
                )
            })?;

            std::io::copy(&mut file, &mut outfile).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to write file {}: {e}", dest_path.display()),
                )
            })?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&dest_path, fs::Permissions::from_mode(mode)).map_err(
                        |e| {
                            Error::new(
                                ErrorKind::InvalidData,
                                format!("Failed to set permissions: {e}"),
                            )
                        },
                    )?;
                }
            }
        }

        extracted.insert(dest_path);
    }

    Ok(extracted)
}

fn get_existing_files(dest: &Path) -> Result<HashSet<PathBuf>> {
    let mut files = HashSet::new();

    if !dest.exists() {
        return Ok(files);
    }

    for entry in walkdir::WalkDir::new(dest) {
        let entry = entry.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to walk directory: {e}"),
            )
        })?;
        files.insert(entry.path().to_path_buf());
    }

    Ok(files)
}

fn run_unarchive(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let dest = PathBuf::from(&params.dest);
    let src = PathBuf::from(&params.src);

    let mut _temp_file: Option<PathBuf> = None;
    let archive_path: PathBuf;

    if params.remote_src {
        let temp_dir = tempfile::tempdir().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create temp dir: {e}"),
            )
        })?;

        let filename = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archive");

        let temp_archive = temp_dir.path().join(filename);
        archive_path = temp_archive.clone();
        _temp_file = Some(temp_dir.keep());

        if !check_mode {
            download_remote_file(&params.src, &archive_path)?;
        }
    } else {
        archive_path = src.clone();

        if !archive_path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Archive file not found: {}", archive_path.display()),
            ));
        }
    }

    if let Some(checksum_param) = &params.checksum
        && !check_mode
    {
        let (algorithm, expected_hash) = parse_checksum(checksum_param)?;
        let actual_hash = calculate_checksum(&archive_path, &algorithm)?;

        if actual_hash != expected_hash {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Checksum verification failed. Expected: {expected_hash}, Got: {actual_hash}"
                ),
            ));
        }
    }

    let exclude = params.exclude.as_deref().unwrap_or(&[]);

    if params.create_dest && !dest.exists() {
        if !check_mode {
            create_dir_all(&dest).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Failed to create destination directory {}: {e}",
                        dest.display()
                    ),
                )
            })?;
        }
        diff(
            "state: absent\n",
            format!("state: directory ({})\n", dest.display()),
        );
    }

    let existing_files = get_existing_files(&dest)?;

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would extract {} to {}",
                archive_path.display(),
                dest.display()
            )),
            extra: None,
        });
    }

    let format = ArchiveFormat::detect_from_path(&archive_path);

    let extracted = if let Some(fmt) = format {
        let file = File::open(&archive_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to open archive {}: {e}", archive_path.display()),
            )
        })?;

        match fmt {
            ArchiveFormat::TarGz => extract_tar_gz(file, &dest, exclude)?,
            ArchiveFormat::TarBz2 => extract_tar_bz2(file, &dest, exclude)?,
            ArchiveFormat::TarXz => extract_tar_xz(file, &dest, exclude)?,
            ArchiveFormat::Tar => extract_tar(file, &dest, exclude)?,
            ArchiveFormat::Zip => {
                let reader = BufReader::new(file);
                extract_zip(reader, &dest, exclude)?
            }
        }
    } else {
        let mut file = File::open(&archive_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to open archive {}: {e}", archive_path.display()),
            )
        })?;

        let mut reader = BufReader::new(&mut file);
        let detected = ArchiveFormat::detect_from_content(&mut reader)?;

        match detected {
            Some(ArchiveFormat::TarGz) => {
                let file = File::open(&archive_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to open archive {}: {e}", archive_path.display()),
                    )
                })?;
                extract_tar_gz(file, &dest, exclude)?
            }
            Some(ArchiveFormat::TarBz2) => {
                let file = File::open(&archive_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to open archive {}: {e}", archive_path.display()),
                    )
                })?;
                extract_tar_bz2(file, &dest, exclude)?
            }
            Some(ArchiveFormat::TarXz) => {
                let file = File::open(&archive_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to open archive {}: {e}", archive_path.display()),
                    )
                })?;
                extract_tar_xz(file, &dest, exclude)?
            }
            Some(ArchiveFormat::Tar) => {
                let file = File::open(&archive_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to open archive {}: {e}", archive_path.display()),
                    )
                })?;
                extract_tar(file, &dest, exclude)?
            }
            Some(ArchiveFormat::Zip) => {
                let file = File::open(&archive_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to open archive {}: {e}", archive_path.display()),
                    )
                })?;
                extract_zip(file, &dest, exclude)?
            }
            None => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Could not detect archive format for {}",
                        archive_path.display()
                    ),
                ));
            }
        }
    };

    let changed = !extracted.is_empty() || existing_files.is_empty();

    if let Some(mode) = &params.mode {
        let mode_int = u32::from_str_radix(mode, 8).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid mode format '{mode}': {e}"),
            )
        })?;

        for path in &extracted {
            set_permissions_recursively(path, mode_int)?;
        }
    }

    let mut new_files: Vec<_> = extracted.difference(&existing_files).collect();
    new_files.sort();

    if !new_files.is_empty() {
        diff(
            "",
            format!(
                "Extracted {} files to {}\n",
                new_files.len(),
                dest.display()
            ),
        );
    }

    Ok(ModuleResult {
        changed,
        output: Some(format!(
            "Extracted {} to {}",
            archive_path.display(),
            dest.display()
        )),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Unarchive;

impl Module for Unarchive {
    fn get_name(&self) -> &str {
        "unarchive"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((run_unarchive(parse_params(params)?, check_mode)?, None))
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

    use flate2::write::GzEncoder;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /tmp/app.tar.gz
            dest: /opt/app
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.src, "/tmp/app.tar.gz");
        assert_eq!(params.dest, "/opt/app");
        assert!(!params.remote_src);
        assert!(params.create_dest);
    }

    #[test]
    fn test_parse_params_with_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: https://example.com/app.tar.gz
            dest: /opt/app
            remote_src: true
            exclude:
              - "*.log"
              - "*.tmp"
            mode: "0755"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.src, "https://example.com/app.tar.gz");
        assert!(params.remote_src);
        assert_eq!(
            params.exclude,
            Some(vec!["*.log".to_string(), "*.tmp".to_string()])
        );
        assert_eq!(params.mode, Some("0755".to_string()));
    }

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("test.log", "*.log"));
        assert!(matches_pattern("file.tmp", "*.tmp"));
        assert!(matches_pattern("dir/test.log", "*.log"));
        assert!(matches_pattern("test", "test"));
        assert!(matches_pattern("path/to/test", "test"));
        assert!(!matches_pattern("test.txt", "*.log"));
        assert!(matches_pattern("file.tar.gz", "*.tar.gz"));
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
    fn test_archive_format_detect() {
        assert_eq!(
            ArchiveFormat::detect_from_path(Path::new("/tmp/test.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::detect_from_path(Path::new("/tmp/test.tgz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::detect_from_path(Path::new("/tmp/test.tar.bz2")),
            Some(ArchiveFormat::TarBz2)
        );
        assert_eq!(
            ArchiveFormat::detect_from_path(Path::new("/tmp/test.tar.xz")),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            ArchiveFormat::detect_from_path(Path::new("/tmp/test.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::detect_from_path(Path::new("/tmp/test.tar")),
            Some(ArchiveFormat::Tar)
        );
        assert_eq!(
            ArchiveFormat::detect_from_path(Path::new("/tmp/test.unknown")),
            None
        );
    }

    #[test]
    fn test_extract_tar_gz_basic() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let archive_path = dir.path().join("test.tar.gz");
        let dest_path = dir.path().join("dest");

        fs::create_dir(&src_dir).unwrap();
        let file_path = src_dir.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "test content").unwrap();

        {
            let file = File::create(&archive_path).unwrap();
            let encoder = GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(encoder);
            tar.append_path_with_name(&file_path, "test.txt").unwrap();
            tar.finish().unwrap();
        }

        fs::create_dir(&dest_path).unwrap();
        let file = File::open(&archive_path).unwrap();
        let extracted = extract_tar_gz(file, &dest_path, &[]).unwrap();

        assert!(extracted.contains(&dest_path.join("test.txt")));
        assert!(dest_path.join("test.txt").exists());

        let contents = fs::read_to_string(dest_path.join("test.txt")).unwrap();
        assert_eq!(contents, "test content\n");
    }

    #[test]
    fn test_extract_zip_basic() {
        let dir = tempdir().unwrap();
        let archive_path = dir.path().join("test.zip");
        let dest_path = dir.path().join("dest");

        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("test.txt", options).unwrap();
            zip.write_all(b"test content").unwrap();
            zip.finish().unwrap();
        }

        fs::create_dir(&dest_path).unwrap();
        let file = File::open(&archive_path).unwrap();
        let reader = BufReader::new(file);
        let extracted = extract_zip(reader, &dest_path, &[]).unwrap();

        assert!(extracted.contains(&dest_path.join("test.txt")));
        assert!(dest_path.join("test.txt").exists());

        let contents = fs::read_to_string(dest_path.join("test.txt")).unwrap();
        assert_eq!(contents, "test content");
    }

    #[test]
    fn test_extract_with_exclude() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let archive_path = dir.path().join("test.tar.gz");
        let dest_path = dir.path().join("dest");

        fs::create_dir(&src_dir).unwrap();
        let file1 = src_dir.join("file.txt");
        let mut f1 = File::create(&file1).unwrap();
        writeln!(f1, "content1").unwrap();
        let file2 = src_dir.join("file.log");
        let mut f2 = File::create(&file2).unwrap();
        writeln!(f2, "log content").unwrap();

        {
            let file = File::create(&archive_path).unwrap();
            let encoder = GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(encoder);
            tar.append_path_with_name(&file1, "file.txt").unwrap();
            tar.append_path_with_name(&file2, "file.log").unwrap();
            tar.finish().unwrap();
        }

        fs::create_dir(&dest_path).unwrap();
        let file = File::open(&archive_path).unwrap();
        let exclude = vec!["*.log".to_string()];
        let extracted = extract_tar_gz(file, &dest_path, &exclude).unwrap();

        assert!(extracted.contains(&dest_path.join("file.txt")));
        assert!(!extracted.contains(&dest_path.join("file.log")));
        assert!(dest_path.join("file.txt").exists());
        assert!(!dest_path.join("file.log").exists());
    }

    #[test]
    fn test_run_unarchive_creates_dest() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let archive_path = dir.path().join("test.tar.gz");
        let dest_path = dir.path().join("dest");

        fs::create_dir(&src_dir).unwrap();
        let file_path = src_dir.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "test").unwrap();

        {
            let file = File::create(&archive_path).unwrap();
            let encoder = GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(encoder);
            tar.append_path_with_name(&file_path, "test.txt").unwrap();
            tar.finish().unwrap();
        }

        let params = Params {
            src: archive_path.to_str().unwrap().to_string(),
            dest: dest_path.to_str().unwrap().to_string(),
            remote_src: false,
            exclude: None,
            mode: None,
            group: None,
            owner: None,
            create_dest: true,
            checksum: None,
        };

        let result = run_unarchive(params, false).unwrap();

        assert!(result.changed);
        assert!(dest_path.exists());
        assert!(dest_path.join("test.txt").exists());
    }

    #[test]
    fn test_run_unarchive_check_mode() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let archive_path = dir.path().join("test.tar.gz");
        let dest_path = dir.path().join("dest");

        fs::create_dir(&src_dir).unwrap();
        let file_path = src_dir.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "test").unwrap();

        {
            let file = File::create(&archive_path).unwrap();
            let encoder = GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(encoder);
            tar.append_path_with_name(&file_path, "test.txt").unwrap();
            tar.finish().unwrap();
        }

        let params = Params {
            src: archive_path.to_str().unwrap().to_string(),
            dest: dest_path.to_str().unwrap().to_string(),
            remote_src: false,
            exclude: None,
            mode: None,
            group: None,
            owner: None,
            create_dest: true,
            checksum: None,
        };

        let result = run_unarchive(params, true).unwrap();

        assert!(result.changed);
        assert!(!dest_path.exists());
    }

    #[test]
    fn test_run_unarchive_missing_src() {
        let dir = tempdir().unwrap();
        let dest_path = dir.path().join("dest");

        let params = Params {
            src: "/nonexistent/archive.tar.gz".to_string(),
            dest: dest_path.to_str().unwrap().to_string(),
            remote_src: false,
            exclude: None,
            mode: None,
            group: None,
            owner: None,
            create_dest: true,
            checksum: None,
        };

        let result = run_unarchive(params, false);
        assert!(result.is_err());
    }
}
