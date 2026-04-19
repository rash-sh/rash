/// ANCHOR: module
/// # syslog
///
/// Send messages to the system syslog daemon.
///
/// This module enables scripts to log messages to the system log daemon,
/// useful for operational logging, debugging, and audit trails in
/// container/IoT environments.
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
/// - name: Log a simple info message
///   syslog:
///     msg: "Script started"
///
/// - name: Log with custom facility and priority
///   syslog:
///     msg: "Critical system error"
///     facility: local0
///     priority: error
///
/// - name: Log with custom identifier
///   syslog:
///     msg: "Container startup complete"
///     ident: myapp
///
/// - name: Log daemon message with PID
///   syslog:
///     msg: "Service heartbeat"
///     facility: daemon
///     priority: info
///     ident: myservice
///     pid: true
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
use std::ffi::CString;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Facility {
    Auth,
    Authpriv,
    Cron,
    Daemon,
    Ftp,
    Kern,
    Local0,
    Local1,
    Local2,
    Local3,
    Local4,
    Local5,
    Local6,
    Local7,
    Lpr,
    Mail,
    News,
    Syslog,
    #[default]
    User,
    Uucp,
}

impl From<Facility> for libc::c_int {
    fn from(facility: Facility) -> Self {
        match facility {
            Facility::Auth => libc::LOG_AUTH,
            Facility::Authpriv => libc::LOG_AUTHPRIV,
            Facility::Cron => libc::LOG_CRON,
            Facility::Daemon => libc::LOG_DAEMON,
            Facility::Ftp => libc::LOG_FTP,
            Facility::Kern => libc::LOG_KERN,
            Facility::Local0 => libc::LOG_LOCAL0,
            Facility::Local1 => libc::LOG_LOCAL1,
            Facility::Local2 => libc::LOG_LOCAL2,
            Facility::Local3 => libc::LOG_LOCAL3,
            Facility::Local4 => libc::LOG_LOCAL4,
            Facility::Local5 => libc::LOG_LOCAL5,
            Facility::Local6 => libc::LOG_LOCAL6,
            Facility::Local7 => libc::LOG_LOCAL7,
            Facility::Lpr => libc::LOG_LPR,
            Facility::Mail => libc::LOG_MAIL,
            Facility::News => libc::LOG_NEWS,
            Facility::Syslog => libc::LOG_SYSLOG,
            Facility::User => libc::LOG_USER,
            Facility::Uucp => libc::LOG_UUCP,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Priority {
    Debug,
    #[default]
    Info,
    Notice,
    Warning,
    Error,
    Crit,
    Alert,
    Emerg,
}

impl From<Priority> for libc::c_int {
    fn from(priority: Priority) -> Self {
        match priority {
            Priority::Debug => libc::LOG_DEBUG,
            Priority::Info => libc::LOG_INFO,
            Priority::Notice => libc::LOG_NOTICE,
            Priority::Warning => libc::LOG_WARNING,
            Priority::Error => libc::LOG_ERR,
            Priority::Crit => libc::LOG_CRIT,
            Priority::Alert => libc::LOG_ALERT,
            Priority::Emerg => libc::LOG_EMERG,
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The message to log to syslog (required).
    msg: String,
    /// The syslog facility to use.
    #[serde(default)]
    facility: Facility,
    /// The priority/severity level of the message.
    #[serde(default)]
    priority: Priority,
    /// Program identifier to use in syslog messages.
    /// Defaults to the script name or "rash".
    #[serde(default)]
    ident: Option<String>,
    /// Include PID in the log message.
    #[serde(default)]
    pid: bool,
}

fn syslog(params: Params) -> Result<ModuleResult> {
    let ident = params.ident.unwrap_or_else(|| "rash".to_string());
    let ident_cstr =
        CString::new(ident.as_str()).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    let facility: libc::c_int = params.facility.into();
    let priority: libc::c_int = params.priority.into();

    let option = if params.pid { libc::LOG_PID } else { 0 };

    unsafe {
        libc::openlog(ident_cstr.as_ptr(), option, facility);
    }

    let msg_cstr =
        CString::new(params.msg.as_str()).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    unsafe {
        libc::syslog(priority, c"%s".as_ptr(), msg_cstr.as_ptr());
        libc::closelog();
    }

    Ok(ModuleResult::new(
        true,
        None,
        Some("Message logged to syslog".to_string()),
    ))
}

#[derive(Debug)]
pub struct Syslog;

impl Module for Syslog {
    fn get_name(&self) -> &str {
        "syslog"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((syslog(parse_params(optional_params)?)?, None))
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
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "test message"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.msg, "test message");
        assert_eq!(params.facility, Facility::User);
        assert_eq!(params.priority, Priority::Info);
        assert_eq!(params.ident, None);
        assert!(!params.pid);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "critical error"
            facility: daemon
            priority: error
            ident: myapp
            pid: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.msg, "critical error");
        assert_eq!(params.facility, Facility::Daemon);
        assert_eq!(params.priority, Priority::Error);
        assert_eq!(params.ident, Some("myapp".to_string()));
        assert!(params.pid);
    }

    #[test]
    fn test_parse_params_all_facilities() {
        let facilities = [
            ("auth", Facility::Auth),
            ("authpriv", Facility::Authpriv),
            ("cron", Facility::Cron),
            ("daemon", Facility::Daemon),
            ("ftp", Facility::Ftp),
            ("kern", Facility::Kern),
            ("local0", Facility::Local0),
            ("local1", Facility::Local1),
            ("local2", Facility::Local2),
            ("local3", Facility::Local3),
            ("local4", Facility::Local4),
            ("local5", Facility::Local5),
            ("local6", Facility::Local6),
            ("local7", Facility::Local7),
            ("lpr", Facility::Lpr),
            ("mail", Facility::Mail),
            ("news", Facility::News),
            ("syslog", Facility::Syslog),
            ("user", Facility::User),
            ("uucp", Facility::Uucp),
        ];

        for (name, expected) in facilities {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                msg: "test"
                facility: {}
                "#,
                name
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(params.facility, expected, "Failed for facility: {}", name);
        }
    }

    #[test]
    fn test_parse_params_all_priorities() {
        let priorities = [
            ("debug", Priority::Debug),
            ("info", Priority::Info),
            ("notice", Priority::Notice),
            ("warning", Priority::Warning),
            ("error", Priority::Error),
            ("crit", Priority::Crit),
            ("alert", Priority::Alert),
            ("emerg", Priority::Emerg),
        ];

        for (name, expected) in priorities {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                msg: "test"
                priority: {}
                "#,
                name
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(params.priority, expected, "Failed for priority: {}", name);
        }
    }

    #[test]
    fn test_parse_params_invalid_facility() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "test"
            facility: invalid
            "#,
        )
        .unwrap();
        let result: Result<Params> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_missing_msg() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            facility: daemon
            "#,
        )
        .unwrap();
        let result: Result<Params> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_facility_to_libc() {
        assert_eq!(libc::c_int::from(Facility::User), libc::LOG_USER);
        assert_eq!(libc::c_int::from(Facility::Daemon), libc::LOG_DAEMON);
        assert_eq!(libc::c_int::from(Facility::Local0), libc::LOG_LOCAL0);
    }

    #[test]
    fn test_priority_to_libc() {
        assert_eq!(libc::c_int::from(Priority::Debug), libc::LOG_DEBUG);
        assert_eq!(libc::c_int::from(Priority::Info), libc::LOG_INFO);
        assert_eq!(libc::c_int::from(Priority::Error), libc::LOG_ERR);
    }
}
