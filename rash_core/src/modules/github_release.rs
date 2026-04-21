/// ANCHOR: module
/// # github_release
///
/// Download release assets from GitHub releases.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: partial
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - github_release:
///     repo: hashicorp/terraform
///     tag: "1.7.0"
///     asset: "terraform_.*_linux_amd64.zip"
///     dest: /usr/local/bin/terraform.zip
///     mode: "0755"
///
/// - github_release:
///     repo: hashicorp/nomad
///     dest: /tmp/nomad
///
/// - github_release:
///     repo: cli/cli
///     tag: latest
///     asset: "gh_.*_linux_amd64.tar.gz"
///     dest: /tmp/gh.tar.gz
///     mode: "0644"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use minijinja::Value;
use regex::Regex;
use reqwest::blocking::Client;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// GitHub repository in owner/repo format
    pub repo: String,
    /// Release tag to download from (default: latest)
    #[serde(default = "default_tag")]
    pub tag: String,
    /// Specific asset name pattern to download (regex supported)
    pub asset: Option<String>,
    /// Destination path for downloaded file
    pub dest: String,
    /// File permissions (default: 0755 for binaries)
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Timeout in seconds for API and download requests
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// GitHub API token for private repositories or higher rate limits
    pub api_token: Option<String>,
}

fn default_tag() -> String {
    "latest".to_string()
}

fn default_mode() -> String {
    "0755".to_string()
}

fn default_timeout() -> u64 {
    60
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    assets: Vec<AssetResponse>,
}

#[derive(Debug, Deserialize)]
struct AssetResponse {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn build_client(timeout: u64) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(timeout))
        .user_agent("rash-github-release-module")
        .build()
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create HTTP client: {e}"),
            )
        })
}

fn fetch_release(
    client: &Client,
    repo: &str,
    tag: &str,
    api_token: Option<&str>,
) -> Result<ReleaseResponse> {
    let url = if tag == "latest" {
        format!("https://api.github.com/repos/{repo}/releases/latest")
    } else {
        format!("https://api.github.com/repos/{repo}/releases/tags/{tag}")
    };

    let mut request = client.get(&url);
    if let Some(token) = api_token {
        request = request.bearer_auth(token);
    }

    let response = request.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("GitHub API request failed for {repo}: {e}"),
        )
    })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("GitHub API returned {status} for {url}: {body}"),
        ));
    }

    response.json::<ReleaseResponse>().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to parse GitHub release response: {e}"),
        )
    })
}

fn find_asset<'a>(release: &'a ReleaseResponse, pattern: &str) -> Result<&'a AssetResponse> {
    let regex = Regex::new(pattern).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Invalid asset pattern '{pattern}': {e}"),
        )
    })?;

    let matches: Vec<&AssetResponse> = release
        .assets
        .iter()
        .filter(|a| regex.is_match(&a.name))
        .collect();

    match matches.len() {
        0 => Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "No asset matching pattern '{pattern}' found in release {}. Available assets: {}",
                release.tag_name,
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
        1 => Ok(matches[0]),
        _ => {
            let names: Vec<&str> = matches.iter().map(|a| a.name.as_str()).collect();
            Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Multiple assets match pattern '{pattern}': {}. Please use a more specific pattern.",
                    names.join(", ")
                ),
            ))
        }
    }
}

fn pick_first_asset(release: &ReleaseResponse) -> Result<&AssetResponse> {
    release.assets.first().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Release {} has no assets", release.tag_name),
        )
    })
}

fn download_asset(
    client: &Client,
    asset: &AssetResponse,
    dest: &Path,
    api_token: Option<&str>,
) -> Result<()> {
    let mut request = client.get(&asset.browser_download_url);
    if let Some(token) = api_token {
        request = request.bearer_auth(token);
    }

    let response = request.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to download asset '{}': {e}", asset.name),
        )
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Download failed with status {status} for '{}'", asset.name),
        ));
    }

    let content = response.bytes().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to read download response: {e}"),
        )
    })?;

    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create parent directories: {e}"),
            )
        })?;
    }

    let mut file = File::create(dest).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to create file '{}': {e}", dest.display()),
        )
    })?;

    file.write_all(&content).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to write file '{}': {e}", dest.display()),
        )
    })?;

    Ok(())
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
    })
}

#[derive(Debug)]
pub struct GithubRelease;

impl Module for GithubRelease {
    fn get_name(&self) -> &str {
        "github_release"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;

        let client = build_client(params.timeout)?;
        let release = fetch_release(
            &client,
            &params.repo,
            &params.tag,
            params.api_token.as_deref(),
        )?;

        let asset = match &params.asset {
            Some(pattern) => find_asset(&release, pattern)?,
            None => pick_first_asset(&release)?,
        };

        let dest_path = Path::new(&params.dest);

        if check_mode {
            return Ok((
                ModuleResult {
                    changed: true,
                    output: Some(format!(
                        "Would download '{}' from {}/{} to {}",
                        asset.name,
                        params.repo,
                        release.tag_name,
                        dest_path.display()
                    )),
                    extra: None,
                },
                None,
            ));
        }

        download_asset(&client, asset, dest_path, params.api_token.as_deref())?;
        set_file_permissions(dest_path, &params.mode)?;

        let extra_data = json!({
            "dest": dest_path.display().to_string(),
            "repo": params.repo,
            "tag": release.tag_name,
            "asset": asset.name,
            "size": asset.size,
            "url": asset.browser_download_url,
            "mode": params.mode,
        });

        Ok((
            ModuleResult {
                changed: true,
                output: Some(format!(
                    "Downloaded '{}' from {}/{} to {}",
                    asset.name,
                    params.repo,
                    release.tag_name,
                    dest_path.display()
                )),
                extra: Some(value::to_value(extra_data)?),
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

    #[test]
    fn test_default_tag() {
        assert_eq!(default_tag(), "latest");
    }

    #[test]
    fn test_default_mode() {
        assert_eq!(default_mode(), "0755");
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 60);
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
repo: "hashicorp/terraform"
dest: "/tmp/terraform"
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repo, "hashicorp/terraform");
        assert_eq!(params.dest, "/tmp/terraform");
        assert_eq!(params.tag, "latest");
        assert_eq!(params.mode, "0755");
        assert_eq!(params.timeout, 60);
        assert!(params.asset.is_none());
        assert!(params.api_token.is_none());
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
repo: "cli/cli"
tag: "v2.40.0"
asset: "gh_.*_linux_amd64.tar.gz"
dest: "/usr/local/bin/gh.tar.gz"
mode: "0644"
timeout: 120
api_token: "ghp_test123"
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.repo, "cli/cli");
        assert_eq!(params.tag, "v2.40.0");
        assert_eq!(params.asset.unwrap(), "gh_.*_linux_amd64.tar.gz");
        assert_eq!(params.dest, "/usr/local/bin/gh.tar.gz");
        assert_eq!(params.mode, "0644");
        assert_eq!(params.timeout, 120);
        assert_eq!(params.api_token.unwrap(), "ghp_test123");
    }

    #[test]
    fn test_parse_params_missing_repo() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
dest: "/tmp/file"
"#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_missing_dest() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
repo: "hashicorp/terraform"
"#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
repo: "hashicorp/terraform"
dest: "/tmp/terraform"
unknown_field: "value"
"#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_asset_exact_match() {
        let release = ReleaseResponse {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                AssetResponse {
                    name: "tool_linux_amd64".to_string(),
                    browser_download_url: "https://example.com/tool_linux_amd64".to_string(),
                    size: 100,
                },
                AssetResponse {
                    name: "tool_windows_amd64.exe".to_string(),
                    browser_download_url: "https://example.com/tool_windows_amd64.exe".to_string(),
                    size: 200,
                },
            ],
        };

        let asset = find_asset(&release, "tool_linux_amd64").unwrap();
        assert_eq!(asset.name, "tool_linux_amd64");
    }

    #[test]
    fn test_find_asset_regex_match() {
        let release = ReleaseResponse {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                AssetResponse {
                    name: "terraform_1.7.0_linux_amd64.zip".to_string(),
                    browser_download_url: "https://example.com/terraform.zip".to_string(),
                    size: 100,
                },
                AssetResponse {
                    name: "terraform_1.7.0_windows_amd64.zip".to_string(),
                    browser_download_url: "https://example.com/terraform_win.zip".to_string(),
                    size: 200,
                },
            ],
        };

        let asset = find_asset(&release, "terraform_.*_linux_amd64.zip").unwrap();
        assert_eq!(asset.name, "terraform_1.7.0_linux_amd64.zip");
    }

    #[test]
    fn test_find_asset_no_match() {
        let release = ReleaseResponse {
            tag_name: "v1.0.0".to_string(),
            assets: vec![AssetResponse {
                name: "tool_linux_amd64".to_string(),
                browser_download_url: "https://example.com/tool".to_string(),
                size: 100,
            }],
        };

        let result = find_asset(&release, "nonexistent_pattern");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_asset_multiple_matches() {
        let release = ReleaseResponse {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                AssetResponse {
                    name: "tool_linux_amd64".to_string(),
                    browser_download_url: "https://example.com/tool1".to_string(),
                    size: 100,
                },
                AssetResponse {
                    name: "tool_linux_arm64".to_string(),
                    browser_download_url: "https://example.com/tool2".to_string(),
                    size: 200,
                },
            ],
        };

        let result = find_asset(&release, "tool_linux_.*");
        assert!(result.is_err());
    }

    #[test]
    fn test_pick_first_asset() {
        let release = ReleaseResponse {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                AssetResponse {
                    name: "first.tar.gz".to_string(),
                    browser_download_url: "https://example.com/first".to_string(),
                    size: 100,
                },
                AssetResponse {
                    name: "second.tar.gz".to_string(),
                    browser_download_url: "https://example.com/second".to_string(),
                    size: 200,
                },
            ],
        };

        let asset = pick_first_asset(&release).unwrap();
        assert_eq!(asset.name, "first.tar.gz");
    }

    #[test]
    fn test_pick_first_asset_empty() {
        let release = ReleaseResponse {
            tag_name: "v1.0.0".to_string(),
            assets: vec![],
        };

        let result = pick_first_asset(&release);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_client() {
        let client = build_client(30);
        assert!(client.is_ok());
    }

    #[test]
    fn test_set_file_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_file");
        File::create(&file_path).unwrap();

        set_file_permissions(&file_path, "0755").unwrap();

        let metadata = fs::metadata(&file_path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }
}
