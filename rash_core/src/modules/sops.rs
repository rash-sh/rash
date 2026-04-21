/// ANCHOR: module
/// # sops
///
/// Encrypt and decrypt files using Mozilla SOPS (Secrets OPerationS).
///
/// This module provides declarative secrets management for GitOps workflows.
/// SOPS supports multiple encryption backends including PGP, AWS KMS, GCP KMS,
/// Azure Key Vault, and age. Backend configuration is typically managed through
/// environment variables or `.sops.yaml` configuration files.
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
/// - name: Decrypt secrets file to a different path
///   sops:
///     src: secrets.enc.yaml
///     dst: secrets.yaml
///     state: decrypted
///
/// - name: Decrypt secrets file in-place
///   sops:
///     src: secrets.enc.yaml
///     state: decrypted
///
/// - name: Encrypt secrets file in-place
///   sops:
///     src: secrets.yaml
///     state: encrypted
///
/// - name: Encrypt with specific input/output types
///   sops:
///     src: secrets.json
///     dst: secrets.enc.json
///     input_type: json
///     output_type: json
///     state: encrypted
///
/// - name: Decrypt with age backend
///   sops:
///     src: secrets.enc.yaml
///     dst: secrets.yaml
///     state: decrypted
///   environment:
///     SOPS_AGE_KEY_FILE: /path/to/key.txt
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Decrypted,
    #[default]
    Encrypted,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the source file to encrypt or decrypt.
    pub src: String,
    /// Path to write the output file. If not specified, the source file is modified in-place.
    pub dst: Option<String>,
    /// The desired state of the file.
    /// `encrypted` encrypts the file, `decrypted` decrypts it.
    /// **[default: `"encrypted"`]**
    #[serde(default)]
    pub state: State,
    /// Input file type (json, yaml, dotenv, binary). Auto-detected if not specified.
    pub input_type: Option<String>,
    /// Output file type (json, yaml, dotenv, binary). Defaults to input_type if not specified.
    pub output_type: Option<String>,
    /// Path to the SOPS binary. Uses `sops` from PATH by default.
    #[serde(default = "default_sops_binary")]
    pub sops_binary: String,
}

fn default_sops_binary() -> String {
    "sops".to_string()
}

fn run_sops_command(args: &[&str], sops_binary: &str) -> Result<std::process::Output> {
    let output = Command::new(sops_binary).args(args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute sops command: {e}. Is sops installed?"),
        )
    })?;

    trace!("sops command: {} {:?}", sops_binary, args);
    trace!("sops output: {:?}", output);

    Ok(output)
}

fn check_file_status(src: &str, sops_binary: &str) -> Result<bool> {
    let output = run_sops_command(&["filestatus", src], sops_binary)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("sops filestatus failed: {stderr}"),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let status: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse sops filestatus output: {e}"),
        )
    })?;

    let mode = status
        .get("mode")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown");

    Ok(mode == "encrypted")
}

fn build_common_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(ref input_type) = params.input_type {
        args.push("--input-type".to_string());
        args.push(input_type.clone());
    }

    if let Some(ref output_type) = params.output_type {
        args.push("--output-type".to_string());
        args.push(output_type.clone());
    }

    args
}

fn exec_decrypt(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let src_path = Path::new(&params.src);
    if !src_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Source file not found: {}", params.src),
        ));
    }

    let is_encrypted = check_file_status(&params.src, &params.sops_binary)?;

    if !is_encrypted {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "src": params.src,
                "state": "decrypted",
                "status": "already_decrypted"
            }))?),
            Some(format!("{} is already decrypted", params.src)),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "src": params.src,
                "dst": params.dst,
                "state": "decrypted",
            }))?),
            Some(format!("Would decrypt {}", params.src)),
        ));
    }

    let mut args = vec!["--decrypt".to_string()];
    args.extend(build_common_args(params));

    if let Some(ref dst) = params.dst {
        args.push("--output".to_string());
        args.push(dst.clone());
    } else {
        args.push("--in-place".to_string());
    }

    args.push(params.src.clone());

    let output = run_sops_command(
        &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        &params.sops_binary,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("sops decrypt failed: {stderr}"),
        ));
    }

    let dst_display = params.dst.as_deref().unwrap_or(&params.src);

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "src": params.src,
            "dst": dst_display,
            "state": "decrypted",
        }))?),
        Some(format!("{} decrypted successfully", params.src)),
    ))
}

fn exec_encrypt(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let src_path = Path::new(&params.src);
    if !src_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Source file not found: {}", params.src),
        ));
    }

    let is_encrypted = check_file_status(&params.src, &params.sops_binary)?;

    if is_encrypted {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "src": params.src,
                "state": "encrypted",
                "status": "already_encrypted"
            }))?),
            Some(format!("{} is already encrypted", params.src)),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "src": params.src,
                "dst": params.dst,
                "state": "encrypted",
            }))?),
            Some(format!("Would encrypt {}", params.src)),
        ));
    }

    let mut args = vec!["--encrypt".to_string()];
    args.extend(build_common_args(params));

    if let Some(ref dst) = params.dst {
        args.push("--output".to_string());
        args.push(dst.clone());
    } else {
        args.push("--in-place".to_string());
    }

    args.push(params.src.clone());

    let output = run_sops_command(
        &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        &params.sops_binary,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("sops encrypt failed: {stderr}"),
        ));
    }

    let dst_display = params.dst.as_deref().unwrap_or(&params.src);

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "src": params.src,
            "dst": dst_display,
            "state": "encrypted",
        }))?),
        Some(format!("{} encrypted successfully", params.src)),
    ))
}

pub fn sops(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Decrypted => exec_decrypt(&params, check_mode),
        State::Encrypted => exec_encrypt(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct Sops;

impl Module for Sops {
    fn get_name(&self) -> &str {
        "sops"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((sops(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_decrypt() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: secrets.enc.yaml
            dst: secrets.yaml
            state: decrypted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.src, "secrets.enc.yaml");
        assert_eq!(params.dst, Some("secrets.yaml".to_string()));
        assert_eq!(params.state, State::Decrypted);
    }

    #[test]
    fn test_parse_params_encrypt_inplace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: secrets.yaml
            state: encrypted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.src, "secrets.yaml");
        assert_eq!(params.dst, None);
        assert_eq!(params.state, State::Encrypted);
    }

    #[test]
    fn test_parse_params_with_types() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: secrets.json
            dst: secrets.enc.json
            input_type: json
            output_type: json
            state: encrypted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.input_type, Some("json".to_string()));
        assert_eq!(params.output_type, Some("json".to_string()));
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: secrets.yaml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.dst, None);
        assert_eq!(params.state, State::Encrypted);
        assert_eq!(params.sops_binary, "sops");
        assert_eq!(params.input_type, None);
        assert_eq!(params.output_type, None);
    }

    #[test]
    fn test_parse_params_custom_binary() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: secrets.yaml
            sops_binary: /usr/local/bin/sops
            state: encrypted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.sops_binary, "/usr/local/bin/sops");
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: secrets.yaml
            unknown_field: value
            state: encrypted
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_state() {
        let state: State = Default::default();
        assert_eq!(state, State::Encrypted);
    }

    #[test]
    fn test_build_common_args_no_types() {
        let params = Params {
            src: "test.yaml".to_string(),
            dst: None,
            state: State::Encrypted,
            input_type: None,
            output_type: None,
            sops_binary: "sops".to_string(),
        };
        let args = build_common_args(&params);
        assert!(args.is_empty());
    }

    #[test]
    fn test_build_common_args_with_input_type() {
        let params = Params {
            src: "test.json".to_string(),
            dst: None,
            state: State::Encrypted,
            input_type: Some("json".to_string()),
            output_type: None,
            sops_binary: "sops".to_string(),
        };
        let args = build_common_args(&params);
        assert_eq!(args, vec!["--input-type", "json"]);
    }

    #[test]
    fn test_build_common_args_with_both_types() {
        let params = Params {
            src: "test.json".to_string(),
            dst: None,
            state: State::Decrypted,
            input_type: Some("json".to_string()),
            output_type: Some("yaml".to_string()),
            sops_binary: "sops".to_string(),
        };
        let args = build_common_args(&params);
        assert_eq!(args, vec!["--input-type", "json", "--output-type", "yaml"]);
    }

    #[test]
    fn test_exec_decrypt_file_not_found() {
        let params = Params {
            src: "/nonexistent/file.yaml".to_string(),
            dst: None,
            state: State::Decrypted,
            input_type: None,
            output_type: None,
            sops_binary: "sops".to_string(),
        };
        let result = exec_decrypt(&params, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_exec_encrypt_file_not_found() {
        let params = Params {
            src: "/nonexistent/file.yaml".to_string(),
            dst: None,
            state: State::Encrypted,
            input_type: None,
            output_type: None,
            sops_binary: "sops".to_string(),
        };
        let result = exec_encrypt(&params, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
