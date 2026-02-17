/// ANCHOR: module
/// # wait_for
///
/// Wait until a TCP port accepts connections or `timeout` is reached.
/// This module fails unless `ignore_errors` is set to `true`.
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
/// - wait_for:
///     port: 8080
///     timeout: 30
///
/// - wait_for:
///     port: 5432
///     connect_timeout: 10
///     timeout: 60
///     ignore_errors: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};
use std::time::{Duration, Instant};

const DEFAULT_CONNECT_TIMEOUT: u64 = 5;
const DEFAULT_SLEEP_MS: u64 = 100;

fn default_connect_timeout() -> u64 {
    DEFAULT_CONNECT_TIMEOUT
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Maximum number of seconds to wait for a connection to happen
    /// before closing and retrying.
    #[serde(default = "default_connect_timeout")]
    connect_timeout: u64,
    /// Port number to poll.
    port: u16,
    /// Maximum number of seconds to wait for.
    timeout: u64,
    /// Host to connect to. Defaults to localhost.
    #[serde(default = "default_host")]
    host: String,
}

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn check_port(host: &str, port: u16, connect_timeout: u64) -> std::io::Result<()> {
    let addr: SocketAddr = SocketAddr::V4(SocketAddrV4::new(
        host.parse::<Ipv4Addr>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?,
        port,
    ));
    TcpStream::connect_timeout(&addr, Duration::from_secs(connect_timeout))?;
    Ok(())
}

fn wait_for_port(params: Params) -> Result<ModuleResult> {
    let start = Instant::now();
    let timeout = Duration::from_secs(params.timeout);
    let sleep_duration = Duration::from_millis(DEFAULT_SLEEP_MS);

    loop {
        match check_port(&params.host, params.port, params.connect_timeout) {
            Ok(_) => {
                return Ok(ModuleResult::new(
                    false,
                    None,
                    Some(params.port.to_string()),
                ));
            }
            Err(e) => {
                if start.elapsed() >= timeout {
                    return Err(Error::new(
                        ErrorKind::SubprocessFail,
                        format!(
                            "Timeout waiting for port {} on {}: {}",
                            params.port, params.host, e
                        ),
                    ));
                }
                std::thread::sleep(sleep_duration);
            }
        }
    }
}

#[derive(Debug)]
pub struct WaitFor;

impl Module for WaitFor {
    fn get_name(&self) -> &str {
        "wait_for"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((wait_for_port(parse_params(optional_params)?)?, None))
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            port: 8080
            timeout: 30
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                port: 8080,
                timeout: 30,
                connect_timeout: DEFAULT_CONNECT_TIMEOUT,
                host: "127.0.0.1".to_owned(),
            }
        );
    }

    #[test]
    fn test_parse_params_with_all_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            port: 5432
            timeout: 60
            connect_timeout: 10
            host: "192.168.1.1"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                port: 5432,
                timeout: 60,
                connect_timeout: 10,
                host: "192.168.1.1".to_owned(),
            }
        );
    }

    #[test]
    fn test_parse_params_missing_required() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            port: 8080
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_wait_for_port_timeout() {
        let params = Params {
            port: 1,
            timeout: 1,
            connect_timeout: 1,
            host: "127.0.0.1".to_owned(),
        };
        let result = wait_for_port(params);
        assert!(result.is_err());
    }
}
