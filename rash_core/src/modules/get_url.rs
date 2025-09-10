/// ANCHOR: module
/// # get_url
///
/// Downloads files from HTTP, HTTPS, or FTP to local destination.
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
/// - get_url:
///     url: http://example.com/path/file.conf
///     dest: /etc/foo.conf
///     mode: '0644'
///
/// - get_url:
///     url: http://example.com/path/file.conf
///     dest: /etc/foo.conf
///     force_basic_auth: true
///     url_username: user
///     url_password: pass
///
/// - get_url:
///     url: http://example.com/path/file.conf
///     dest: /etc/foo.conf
///     headers:
///       User-Agent: "custom-agent"
///       X-Custom-Header: "value"
///
/// - get_url:
///     url: http://example.com/path/file.conf
///     dest: /etc/foo.conf
///     checksum: sha256:b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c
///
/// - get_url:
///     url: http://example.com/path/file.conf
///     dest: /etc/foo.conf
///     backup: true
///     force: true
///
/// - get_url:
///     url: http://example.com/path/file.conf
///     dest: /tmp/
///     timeout: 30
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff_files;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use minijinja::Value;
use reqwest::blocking::{Client, Response};
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use sha2::{Digest, Sha256};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// HTTP, HTTPS, or FTP URL to download
    pub url: String,
    /// Absolute path where to download the file to
    pub dest: String,
    /// Create a backup file including the timestamp information
    #[serde(default)]
    pub backup: bool,
    /// If a checksum is passed, the digest of the destination file will be calculated after download
    pub checksum: Option<String>,
    /// If true, will download the file every time and replace if contents change
    #[serde(default)]
    pub force: bool,
    /// Force the sending of the Basic authentication header upon initial request
    #[serde(default)]
    pub force_basic_auth: bool,
    /// Name of the group that should own the file
    pub group: Option<String>,
    /// Add custom HTTP headers to a request
    pub headers: Option<HashMap<String, String>>,
    /// The permissions the resulting file should have
    pub mode: Option<String>,
    /// Name of the user that should own the file
    pub owner: Option<String>,
    /// Timeout in seconds for URL request
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// A username for HTTP basic authentication
    pub url_username: Option<String>,
    /// A password for HTTP basic authentication
    pub url_password: Option<String>,
    /// If false, SSL certificates will not be validated
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

fn default_timeout() -> u64 {
    10
}

fn default_validate_certs() -> bool {
    true
}

fn calculate_file_checksum(path: &Path, algorithm: &str) -> Result<String> {
    let contents = fs::read(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read file for checksum: {e}"),
        )
    })?;

    match algorithm.to_lowercase().as_str() {
        "sha256" => {
            let mut hasher = Sha256::new();
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

fn create_backup(file_path: &Path) -> Result<Option<String>> {
    if !file_path.exists() {
        return Ok(None);
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let backup_path = format!("{}.{}", file_path.display(), timestamp);
    fs::copy(file_path, &backup_path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create backup: {e}"),
        )
    })?;

    Ok(Some(backup_path))
}

fn make_request(params: &Params) -> Result<Response> {
    let client = Client::builder()
        .timeout(Duration::from_secs(params.timeout))
        .danger_accept_invalid_certs(!params.validate_certs)
        .build()
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create HTTP client: {e}"),
            )
        })?;

    let mut request_builder = client.get(&params.url);

    // Add headers
    if let Some(headers) = &params.headers {
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }
    }

    // Add basic auth
    if let (Some(username), Some(password)) = (&params.url_username, &params.url_password) {
        request_builder = request_builder.basic_auth(username, Some(password));
    }

    let response = request_builder.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("HTTP request failed: {e}"),
        )
    })?;

    if !response.status().is_success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("HTTP request failed with status: {}", response.status()),
        ));
    }

    Ok(response)
}

fn set_file_permissions(path: &Path, mode: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode_int = u32::from_str_radix(mode, 8).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Invalid mode format '{mode}': {e}"),
        )
    })?;

    let permissions = std::fs::Permissions::from_mode(mode_int);
    fs::set_permissions(path, permissions).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to set file permissions: {e}"),
        )
    })?;

    Ok(())
}

fn get_file_metadata(path: &Path) -> Result<serde_json::Value> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to get file metadata: {e}"),
        )
    })?;

    Ok(json!({
        "size": metadata.len(),
        "mode": format!("{:o}", metadata.mode() & 0o777),
        "uid": metadata.uid(),
        "gid": metadata.gid(),
    }))
}

#[derive(Debug)]
pub struct GetUrl;

impl Module for GetUrl {
    fn get_name(&self) -> &str {
        "get_url"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;

        // Parse destination path
        let dest_path = PathBuf::from(&params.dest);
        let is_dest_dir = dest_path.is_dir();

        // Determine the actual file path
        let file_path = if is_dest_dir {
            // Extract filename from URL
            let url_path = reqwest::Url::parse(&params.url)
                .map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Invalid URL '{}': {}", params.url, e),
                    )
                })?
                .path()
                .to_string();

            let filename = Path::new(&url_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("downloaded_file")
                .to_string();

            dest_path.join(filename)
        } else {
            dest_path
        };

        // Check if file exists and if we should skip download
        let file_exists = file_path.exists();
        let mut should_download = params.force || !file_exists;
        let mut backup_file = None;

        // Check existing file checksum if provided
        if let Some(checksum_param) = &params.checksum
            && file_exists
        {
            let (algorithm, expected_hash) = parse_checksum(checksum_param)?;
            let actual_hash = calculate_file_checksum(&file_path, &algorithm)?;

            if actual_hash == expected_hash && !params.force {
                should_download = false;
            }
        }

        if check_mode {
            let changed = should_download;
            return Ok((
                ModuleResult {
                    changed,
                    output: Some(format!(
                        "Would download {} to {}",
                        params.url,
                        file_path.display()
                    )),
                    extra: None,
                },
                None,
            ));
        }

        if !should_download {
            return Ok((
                ModuleResult {
                    changed: false,
                    output: Some(format!(
                        "File {} already exists and is up to date",
                        file_path.display()
                    )),
                    extra: None,
                },
                None,
            ));
        }

        // Create backup if requested and file exists
        if params.backup && file_exists {
            backup_file = create_backup(&file_path)?;
        }

        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to create parent directories: {e}"),
                )
            })?;
        }

        // Download the file
        let response = make_request(&params)?;
        let content = response.bytes().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read response body: {e}"),
            )
        })?;

        // Read existing file content for diff if file exists
        let old_content = if file_exists {
            fs::read_to_string(&file_path).unwrap_or_else(|_| String::new())
        } else {
            String::new()
        };

        // Convert new content to string for diff (if it's valid UTF-8)
        let new_content = String::from_utf8_lossy(&content).to_string();

        // Show diff only if content has actually changed
        if old_content != new_content {
            diff_files(&old_content, &new_content);
        }

        // Write to file
        let mut file = File::create(&file_path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create file: {e}"),
            )
        })?;

        file.write_all(&content).map_err(|e| {
            Error::new(ErrorKind::InvalidData, format!("Failed to write file: {e}"))
        })?;

        // Set file permissions if specified
        if let Some(mode) = &params.mode {
            set_file_permissions(&file_path, mode)?;
        }

        // Verify checksum if provided
        if let Some(checksum_param) = &params.checksum {
            let (algorithm, expected_hash) = parse_checksum(checksum_param)?;
            let actual_hash = calculate_file_checksum(&file_path, &algorithm)?;

            if actual_hash != expected_hash {
                // Clean up the downloaded file
                let _ = fs::remove_file(&file_path);
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Checksum verification failed. Expected: {expected_hash}, Got: {actual_hash}"
                    ),
                ));
            }
        }

        // Prepare result data
        let file_metadata = get_file_metadata(&file_path)?;
        let mut extra_data = json!({
            "dest": file_path.display().to_string(),
            "url": params.url,
            "size": file_metadata["size"],
            "mode": file_metadata["mode"],
            "uid": file_metadata["uid"],
            "gid": file_metadata["gid"],
            "state": "file",
            "status_code": 200,
        });

        if let Some(backup_path) = backup_file {
            extra_data["backup_file"] = json!(backup_path);
        }

        if let Some(checksum_param) = &params.checksum {
            let (algorithm, _) = parse_checksum(checksum_param)?;
            let checksum = calculate_file_checksum(&file_path, &algorithm)?;
            extra_data["checksum_dest"] = json!(checksum);
            extra_data["checksum_src"] = json!(checksum);
        }

        let extra = Some(value::to_value(extra_data)?);

        Ok((
            ModuleResult {
                changed: true,
                output: Some(format!(
                    "File downloaded successfully to {}",
                    file_path.display()
                )),
                extra,
            },
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_norway::from_str;

    #[test]
    fn test_parse_params_simple() {
        let yaml = r#"
url: "http://example.com/file.txt"
dest: "/tmp/downloaded_file.txt"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.url, "http://example.com/file.txt");
        assert_eq!(params.dest, "/tmp/downloaded_file.txt");
        assert_eq!(params.timeout, 10);
        assert!(!params.backup);
        assert!(!params.force);
        assert!(params.validate_certs);
    }

    #[test]
    fn test_parse_params_with_auth() {
        let yaml = r#"
url: "http://example.com/file.txt"
dest: "/tmp/downloaded_file.txt"
url_username: "testuser"
url_password: "testpass"
force_basic_auth: true
timeout: 30
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.url_username.unwrap(), "testuser");
        assert_eq!(params.url_password.unwrap(), "testpass");
        assert!(params.force_basic_auth);
        assert_eq!(params.timeout, 30);
    }

    #[test]
    fn test_parse_params_with_checksum() {
        let yaml = r#"
url: "http://example.com/file.txt"
dest: "/tmp/downloaded_file.txt"
checksum: "sha256:b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c"
backup: true
force: true
mode: "0644"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(
            params.checksum.unwrap(),
            "sha256:b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c"
        );
        assert!(params.backup);
        assert!(params.force);
        assert_eq!(params.mode.unwrap(), "0644");
    }

    #[test]
    fn test_parse_checksum() {
        let result = parse_checksum("sha256:abc123").unwrap();
        assert_eq!(result.0, "sha256");
        assert_eq!(result.1, "abc123");

        let result = parse_checksum("invalid");
        assert!(result.is_err());
    }
}
