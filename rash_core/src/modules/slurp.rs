/// ANCHOR: module
/// # slurp
///
/// This module reads a file and returns its content base64 encoded.
/// Useful for reading files (including binary) for use in templates or registering variables.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Read SSL certificate
///   slurp:
///     src: /etc/ssl/certs/app.crt
///   register: cert_content
///
/// - name: Display certificate info
///   debug:
///     msg: "Certificate: {{ cert_content.content | b64decode }}"
///
/// - name: Read JSON config
///   slurp:
///     src: /etc/app/config.json
///   register: config_raw
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::read;

use base64::{Engine as _, engine::general_purpose};
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The file to read.
    src: String,
}

fn slurp(params: Params) -> Result<ModuleResult> {
    let content = read(&params.src).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to read file '{}': {}", params.src, e),
        )
    })?;

    let encoded = general_purpose::STANDARD.encode(&content);

    let extra = serde_norway::to_value(SlurpResult {
        content: encoded,
        source: params.src,
        encoding: "base64".to_owned(),
    })
    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    Ok(ModuleResult {
        changed: false,
        output: None,
        extra: Some(extra),
    })
}

#[derive(Debug, Serialize)]
struct SlurpResult {
    content: String,
    source: String,
    encoding: String,
}

#[derive(Debug)]
pub struct Slurp;

impl Module for Slurp {
    fn get_name(&self) -> &str {
        "slurp"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((slurp(parse_params(optional_params)?)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /etc/hosts
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/etc/hosts".to_owned(),
            }
        );
    }

    #[test]
    fn test_slurp_text_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let content = "Hello, World!";
        write(&temp_file, content).unwrap();

        let params = Params {
            src: temp_file.path().to_str().unwrap().to_owned(),
        };

        let result = slurp(params).unwrap();
        assert!(!result.get_changed());

        let extra = result.get_extra().unwrap();
        let result_map: serde_norway::Mapping = extra.as_mapping().unwrap().clone();
        let encoded = result_map
            .get(YamlValue::String("content".to_owned()))
            .unwrap()
            .as_str()
            .unwrap();
        let source = result_map
            .get(YamlValue::String("source".to_owned()))
            .unwrap()
            .as_str()
            .unwrap();

        let decoded = general_purpose::STANDARD.decode(encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), content);
        assert_eq!(source, temp_file.path().to_str().unwrap());
    }

    #[test]
    fn test_slurp_binary_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let content: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD];
        write(&temp_file, &content).unwrap();

        let params = Params {
            src: temp_file.path().to_str().unwrap().to_owned(),
        };

        let result = slurp(params).unwrap();
        assert!(!result.get_changed());

        let extra = result.get_extra().unwrap();
        let result_map: serde_norway::Mapping = extra.as_mapping().unwrap().clone();
        let encoded = result_map
            .get(YamlValue::String("content".to_owned()))
            .unwrap()
            .as_str()
            .unwrap();

        let decoded = general_purpose::STANDARD.decode(encoded).unwrap();
        assert_eq!(decoded, content);
    }

    #[test]
    fn test_slurp_nonexistent_file() {
        let params = Params {
            src: "/nonexistent/file.txt".to_owned(),
        };

        let result = slurp(params);
        assert!(result.is_err());
    }
}
