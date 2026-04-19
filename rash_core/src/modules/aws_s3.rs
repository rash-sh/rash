/// ANCHOR: module
/// # aws_s3
///
/// Manage AWS S3 objects for cloud storage operations.
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
/// - name: Download file from S3
///   aws_s3:
///     bucket: my-bucket
///     object: config/app.yaml
///     dest: /etc/app/config.yaml
///     mode: get
///
/// - name: Upload file to S3
///   aws_s3:
///     bucket: my-bucket
///     object: backups/data.tar.gz
///     src: /tmp/data.tar.gz
///     mode: put
///
/// - name: Delete object from S3
///   aws_s3:
///     bucket: my-bucket
///     object: old/backup.tar.gz
///     mode: delete
///
/// - name: List objects in bucket
///   aws_s3:
///     bucket: my-bucket
///     mode: list
///   register: s3_objects
///
/// - name: Upload with custom endpoint (MinIO)
///   aws_s3:
///     bucket: my-bucket
///     object: data/file.txt
///     src: /tmp/file.txt
///     mode: put
///     endpoint: http://minio:9000
///
/// - name: Download with specific region
///   aws_s3:
///     bucket: my-bucket
///     object: config.yaml
///     dest: /tmp/config.yaml
///     mode: get
///     region: us-west-2
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::env;
use std::fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::primitives::ByteStream;
use md5::{Digest, Md5};

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Get,
    Put,
    Delete,
    List,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The S3 bucket name.
    pub bucket: String,
    /// The S3 object key path.
    pub object: Option<String>,
    /// Local file path to upload (required for mode=put).
    pub src: Option<String>,
    /// Local file path to download (required for mode=get).
    pub dest: Option<String>,
    /// The operation mode: get, put, delete, or list.
    #[serde(default)]
    pub mode: Mode,
    /// AWS region. If not provided, uses AWS_REGION or AWS_DEFAULT_REGION environment variable.
    pub region: Option<String>,
    /// Custom S3 endpoint URL (for S3-compatible services like MinIO).
    pub endpoint: Option<String>,
    /// AWS access key ID. If not provided, uses AWS_ACCESS_KEY_ID environment variable.
    pub access_key: Option<String>,
    /// AWS secret access key. If not provided, uses AWS_SECRET_ACCESS_KEY environment variable.
    pub secret_key: Option<String>,
    /// Prefix to filter objects when listing.
    pub prefix: Option<String>,
    /// Maximum number of objects to return when listing.
    #[serde(default = "default_max_keys")]
    pub max_keys: i32,
}

fn default_max_keys() -> i32 {
    1000
}

fn get_region(params: &Params) -> Option<Region> {
    params
        .region
        .as_ref()
        .map(|r| Region::new(r.clone()))
        .or_else(|| {
            env::var("AWS_REGION")
                .or_else(|_| env::var("AWS_DEFAULT_REGION"))
                .ok()
                .map(Region::new)
        })
}

async fn create_client(params: &Params) -> Result<Client> {
    let region = get_region(params);

    let mut config_loader = aws_config::defaults(BehaviorVersion::latest());

    if let Some(r) = region {
        config_loader = config_loader.region(r);
    }

    if let (Some(access_key), Some(secret_key)) = (&params.access_key, &params.secret_key) {
        let creds = Credentials::new(access_key.clone(), secret_key.clone(), None, None, "params");
        config_loader = config_loader.credentials_provider(creds);
    }

    let base_config = config_loader.load().await;

    let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&base_config);

    if let Some(endpoint) = &params.endpoint {
        s3_config_builder = s3_config_builder
            .endpoint_url(endpoint)
            .force_path_style(true);
    }

    let s3_config = s3_config_builder.build();

    Ok(Client::from_conf(s3_config))
}

fn validate_params(params: &Params) -> Result<()> {
    match params.mode {
        Mode::Get => {
            if params.object.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "object parameter is required for mode=get",
                ));
            }
            if params.dest.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "dest parameter is required for mode=get",
                ));
            }
        }
        Mode::Put => {
            if params.object.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "object parameter is required for mode=put",
                ));
            }
            if params.src.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "src parameter is required for mode=put",
                ));
            }
        }
        Mode::Delete => {
            if params.object.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "object parameter is required for mode=delete",
                ));
            }
        }
        Mode::List => {}
    }
    Ok(())
}

fn compute_file_md5(path: &Path) -> Result<String> {
    let data = fs::read(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read file for checksum: {e}"),
        )
    })?;
    let mut hasher = Md5::new();
    hasher.update(&data);
    Ok(format!("{:x}", hasher.finalize()))
}

async fn exec_get(client: &Client, params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let object = params
        .object
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "object parameter is required"))?;
    let dest = params
        .dest
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "dest parameter is required"))?;

    let dest_path = Path::new(dest);

    let head_result = client
        .head_object()
        .bucket(&params.bucket)
        .key(object)
        .send()
        .await;

    match head_result {
        Ok(head) => {
            let object_size = head.content_length().unwrap_or(0);
            let etag = head.e_tag().map(|s| s.to_string());

            if dest_path.exists() {
                let local_size = fs::metadata(dest_path).map(|m| m.len()).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to get local file metadata: {e}"),
                    )
                })?;

                if local_size == object_size as u64
                    && let Some(ref remote_etag) = etag
                {
                    let clean_etag = remote_etag.trim_matches('"');
                    if !clean_etag.contains('-')
                        && let Ok(local_md5) = compute_file_md5(dest_path)
                        && clean_etag == local_md5
                    {
                        return Ok(ModuleResult::new(
                            false,
                            Some(value::to_value(json!({
                                "bucket": params.bucket,
                                "object": object,
                                "dest": dest,
                                "size": object_size,
                                "etag": etag,
                                "msg": "File already exists with same checksum"
                            }))?),
                            Some("File already exists, skipped".to_string()),
                        ));
                    }
                }
            }

            if check_mode {
                return Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({
                        "bucket": params.bucket,
                        "object": object,
                        "dest": dest,
                        "size": object_size,
                        "msg": "Would download object"
                    }))?),
                    Some(format!("Would download {} to {}", object, dest)),
                ));
            }

            if let Some(parent) = dest_path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to create parent directories: {e}"),
                    )
                })?;
            }

            let response = client
                .get_object()
                .bucket(&params.bucket)
                .key(object)
                .send()
                .await
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to get S3 object: {e}"),
                    )
                })?;

            let etag = response.e_tag().map(|s| s.to_string());

            let body = response.body.collect().await.map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to read S3 object body: {e}"),
                )
            })?;

            let body_bytes = body.into_bytes();

            fs::write(dest_path, &body_bytes).map_err(|e| {
                Error::new(ErrorKind::InvalidData, format!("Failed to write file: {e}"))
            })?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "bucket": params.bucket,
                    "object": object,
                    "dest": dest,
                    "size": body_bytes.len(),
                    "etag": etag
                }))?),
                Some(format!("Downloaded {} to {}", object, dest)),
            ))
        }
        Err(e) => {
            if e.as_service_error()
                .is_some_and(HeadObjectError::is_not_found)
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Object {} not found in bucket {}", object, params.bucket),
                ));
            }
            Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to check object: {e}"),
            ))
        }
    }
}

async fn exec_put(client: &Client, params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let object = params
        .object
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "object parameter is required"))?;
    let src = params
        .src
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "src parameter is required"))?;

    let src_path = Path::new(src);
    if !src_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Source file {} does not exist", src),
        ));
    }

    let file_size = fs::metadata(src_path).map(|m| m.len()).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to get source file metadata: {e}"),
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "bucket": params.bucket,
                "object": object,
                "src": src,
                "size": file_size,
                "msg": "Would upload object"
            }))?),
            Some(format!("Would upload {} to {}", src, object)),
        ));
    }

    let body = ByteStream::from_path(src_path).await.map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read source file: {e}"),
        )
    })?;

    let response = client
        .put_object()
        .bucket(&params.bucket)
        .key(object)
        .body(body)
        .send()
        .await
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to put S3 object: {e}"),
            )
        })?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "bucket": params.bucket,
            "object": object,
            "src": src,
            "size": file_size,
            "etag": response.e_tag().map(|s| s.to_string())
        }))?),
        Some(format!("Uploaded {} to {}", src, object)),
    ))
}

async fn exec_delete(client: &Client, params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let object = params
        .object
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "object parameter is required"))?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "bucket": params.bucket,
                "object": object,
                "msg": "Would delete object"
            }))?),
            Some(format!("Would delete {}", object)),
        ));
    }

    let head_result = client
        .head_object()
        .bucket(&params.bucket)
        .key(object)
        .send()
        .await;

    match head_result {
        Ok(_) => {
            client
                .delete_object()
                .bucket(&params.bucket)
                .key(object)
                .send()
                .await
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to delete S3 object: {e}"),
                    )
                })?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "bucket": params.bucket,
                    "object": object
                }))?),
                Some(format!("Deleted {}", object)),
            ))
        }
        Err(e) => {
            if e.as_service_error()
                .is_some_and(HeadObjectError::is_not_found)
            {
                return Ok(ModuleResult::new(
                    false,
                    Some(value::to_value(json!({
                        "bucket": params.bucket,
                        "object": object,
                        "found": false
                    }))?),
                    Some(format!("Object {} not found, already deleted", object)),
                ));
            }
            Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to check object: {e}"),
            ))
        }
    }
}

async fn exec_list(client: &Client, params: &Params, _check_mode: bool) -> Result<ModuleResult> {
    let mut request = client
        .list_objects_v2()
        .bucket(&params.bucket)
        .max_keys(params.max_keys);

    if let Some(prefix) = &params.prefix {
        request = request.prefix(prefix);
    }

    let response = request.send().await.map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to list S3 objects: {e}"),
        )
    })?;

    let objects: Vec<serde_json::Value> = response
        .contents()
        .iter()
        .map(|obj| {
            json!({
                "key": obj.key().map(|k| k.to_string()).unwrap_or_default(),
                "size": obj.size().unwrap_or(0),
                "last_modified": obj.last_modified().map(|dt| dt.to_string()).unwrap_or_default(),
                "etag": obj.e_tag().map(|s| s.to_string()).unwrap_or_default(),
            })
        })
        .collect();

    let count = objects.len();
    let truncated = response.is_truncated();

    Ok(ModuleResult::new(
        false,
        Some(value::to_value(json!({
            "bucket": params.bucket,
            "objects": objects,
            "count": count,
            "truncated": truncated,
            "prefix": params.prefix,
        }))?),
        Some(format!(
            "Found {} objects in bucket {}",
            count, params.bucket
        )),
    ))
}

pub async fn aws_s3(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    validate_params(&params)?;

    let client = create_client(&params).await?;

    match params.mode {
        Mode::Get => exec_get(&client, &params, check_mode).await,
        Mode::Put => exec_put(&client, &params, check_mode).await,
        Mode::Delete => exec_delete(&client, &params, check_mode).await,
        Mode::List => exec_list(&client, &params, check_mode).await,
    }
}

#[derive(Debug)]
pub struct AwsS3;

impl Module for AwsS3 {
    fn get_name(&self) -> &str {
        "aws_s3"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;

        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create tokio runtime: {e}"),
            )
        })?;

        let result = rt.block_on(aws_s3(params, check_mode))?;

        Ok((result, None))
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
    fn test_parse_params_get() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            object: "config.yaml"
            dest: "/tmp/config.yaml"
            mode: get
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.bucket, "my-bucket");
        assert_eq!(params.object, Some("config.yaml".to_string()));
        assert_eq!(params.dest, Some("/tmp/config.yaml".to_string()));
        assert_eq!(params.mode, Mode::Get);
    }

    #[test]
    fn test_parse_params_put() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            object: "backup.tar.gz"
            src: "/tmp/backup.tar.gz"
            mode: put
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Mode::Put);
        assert_eq!(params.src, Some("/tmp/backup.tar.gz".to_string()));
    }

    #[test]
    fn test_parse_params_delete() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            object: "old-file.txt"
            mode: delete
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Mode::Delete);
    }

    #[test]
    fn test_parse_params_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            mode: list
            prefix: "backup/"
            max_keys: 100
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Mode::List);
        assert_eq!(params.prefix, Some("backup/".to_string()));
        assert_eq!(params.max_keys, 100);
    }

    #[test]
    fn test_parse_params_with_region() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            object: "file.txt"
            dest: "/tmp/file.txt"
            mode: get
            region: "us-west-2"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.region, Some("us-west-2".to_string()));
    }

    #[test]
    fn test_parse_params_with_endpoint() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            object: "file.txt"
            src: "/tmp/file.txt"
            mode: put
            endpoint: "http://minio:9000"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.endpoint, Some("http://minio:9000".to_string()));
    }

    #[test]
    fn test_parse_params_with_credentials() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            object: "file.txt"
            dest: "/tmp/file.txt"
            mode: get
            access_key: "AKIAIOSFODNN7EXAMPLE"
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.access_key, Some("AKIAIOSFODNN7EXAMPLE".to_string()));
        assert_eq!(
            params.secret_key,
            Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string())
        );
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bucket: "my-bucket"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Mode::Get);
        assert_eq!(params.max_keys, 1000);
    }

    #[test]
    fn test_validate_params_get_missing_object() {
        let params = Params {
            bucket: "test".to_string(),
            object: None,
            dest: Some("/tmp/file".to_string()),
            mode: Mode::Get,
            ..Default::default()
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_get_missing_dest() {
        let params = Params {
            bucket: "test".to_string(),
            object: Some("file".to_string()),
            dest: None,
            mode: Mode::Get,
            ..Default::default()
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_put_missing_src() {
        let params = Params {
            bucket: "test".to_string(),
            object: Some("file".to_string()),
            src: None,
            mode: Mode::Put,
            ..Default::default()
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_delete_missing_object() {
        let params = Params {
            bucket: "test".to_string(),
            object: None,
            mode: Mode::Delete,
            ..Default::default()
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_list_valid() {
        let params = Params {
            bucket: "test".to_string(),
            mode: Mode::List,
            ..Default::default()
        };
        assert!(validate_params(&params).is_ok());
    }
}
