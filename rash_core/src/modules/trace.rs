/// ANCHOR: module
/// # trace
///
/// Trace system activity using eBPF via bpftrace.
///
/// This module provides pre-built probes for common tracing scenarios without
/// requiring bpftrace knowledge. For advanced use cases, custom bpftrace
/// expressions can be provided.
///
/// ## Prerequisites
///
/// - `bpftrace` must be installed and available in PATH
/// - Root privileges (via `become: true`) are typically required
///
/// ## Return Values
///
/// When registered, the following fields are available:
///
/// - `extra.events`: List of captured events
/// - `extra.stats.total`: Total number of events
/// - `extra.stats.by_comm`: Event count grouped by command name
/// - `extra.duration_ms`: Actual trace duration in milliseconds
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ### Trace file opens during startup
///
/// ```yaml
/// - trace:
///     probe: file_opens
///     duration: 10s
///   register: files
///
/// - debug:
///     msg: "Files opened: {{ files.extra.events | length }}"
/// ```
///
/// ### Trace process execution
///
/// ```yaml
/// - trace:
///     probe: process_exec
///     duration: 5s
///   become: true
///   register: procs
/// ```
///
/// ### Filter syscalls
///
/// ```yaml
/// - trace:
///     probe: syscalls
///     filter: open,openat,read,write
///     duration: 10s
///   register: syscalls
/// ```
///
/// ### Custom bpftrace expression
///
/// ```yaml
/// - trace:
///     expr: 'tracepoint:syscalls:sys_enter_open { @[comm] = count(); }'
///     duration: 10s
///   become: true
///   register: custom
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command as StdCommand;
use std::time::Duration;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use strum_macros::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Deserialize, EnumString, Display)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Probe {
    #[strum(serialize = "file_opens")]
    #[cfg_attr(feature = "docs", schemars(rename = "file_opens"))]
    FileOpens,
    #[strum(serialize = "file_reads")]
    #[cfg_attr(feature = "docs", schemars(rename = "file_reads"))]
    FileReads,
    #[strum(serialize = "file_writes")]
    #[cfg_attr(feature = "docs", schemars(rename = "file_writes"))]
    FileWrites,
    #[strum(serialize = "process_exec")]
    #[cfg_attr(feature = "docs", schemars(rename = "process_exec"))]
    ProcessExec,
    #[strum(serialize = "process_exit")]
    #[cfg_attr(feature = "docs", schemars(rename = "process_exit"))]
    ProcessExit,
    #[strum(serialize = "network_connect")]
    #[cfg_attr(feature = "docs", schemars(rename = "network_connect"))]
    NetworkConnect,
    #[strum(serialize = "network_accept")]
    #[cfg_attr(feature = "docs", schemars(rename = "network_accept"))]
    NetworkAccept,
    #[strum(serialize = "syscalls")]
    #[cfg_attr(feature = "docs", schemars(rename = "syscalls"))]
    Syscalls,
}

impl Probe {
    pub fn get_bpftrace_program(&self, filter: Option<&str>) -> String {
        match self {
            Probe::FileOpens => r#"
                tracepoint:syscalls:sys_enter_openat
                {
                    printf("{\"comm\": \"%s\", \"pid\": %d, \"path\": \"%s\", \"timestamp\": %llu}\n",
                        comm, pid, str(args->filename), nsecs / 1000000);
                }
            "#
            .to_string(),
            Probe::FileReads => r#"
                tracepoint:syscalls:sys_enter_read
                {
                    printf("{\"comm\": \"%s\", \"pid\": %d, \"fd\": %d, \"bytes\": %d, \"timestamp\": %llu}\n",
                        comm, pid, args->fd, args->count, nsecs / 1000000);
                }
            "#
            .to_string(),
            Probe::FileWrites => r#"
                tracepoint:syscalls:sys_enter_write
                {
                    printf("{\"comm\": \"%s\", \"pid\": %d, \"fd\": %d, \"bytes\": %d, \"timestamp\": %llu}\n",
                        comm, pid, args->fd, args->count, nsecs / 1000000);
                }
            "#
            .to_string(),
            Probe::ProcessExec => r#"
                tracepoint:syscalls:sys_enter_execve
                {
                    printf("{\"comm\": \"%s\", \"pid\": %d, \"ppid\": %d, \"args\": \"%s\", \"timestamp\": %llu}\n",
                        comm, pid, ppid, str(args->argv), nsecs / 1000000);
                }
            "#
            .to_string(),
            Probe::ProcessExit => r#"
                tracepoint:sched:sched_process_exit
                {
                    printf("{\"comm\": \"%s\", \"pid\": %d, \"exit_code\": %d, \"timestamp\": %llu}\n",
                        comm, pid, args->exit_code, nsecs / 1000000);
                }
            "#
            .to_string(),
            Probe::NetworkConnect => r#"
                tracepoint:syscalls:sys_enter_connect
                {
                    printf("{\"comm\": \"%s\", \"pid\": %d, \"fd\": %d, \"timestamp\": %llu}\n",
                        comm, pid, args->fd, nsecs / 1000000);
                }
            "#
            .to_string(),
            Probe::NetworkAccept => r#"
                tracepoint:syscalls:sys_enter_accept4
                {
                    printf("{\"comm\": \"%s\", \"pid\": %d, \"fd\": %d, \"timestamp\": %llu}\n",
                        comm, pid, args->fd, nsecs / 1000000);
                }
            "#
            .to_string(),
            Probe::Syscalls => {
                let syscall_filter = filter.map_or(String::new(), |f| {
                    let syscalls: Vec<&str> = f.split(',').map(|s| s.trim()).collect();
                    if syscalls.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "/ {} /",
                            syscalls
                                .iter()
                                .map(|s| format!("syscall == \"{}\"", s))
                                .collect::<Vec<_>>()
                                .join(" || ")
                        )
                    }
                });
                let template = r#"
                    tracepoint:raw_syscalls:sys_enter FILTER
                    {
                        printf("{\"comm\": \"%s\", \"pid\": %d, \"syscall\": \"%s\", \"timestamp\": %llu}\n",
                            comm, pid, args->id, nsecs / 1000000);
                    }
                "#;
                template.replace("FILTER", &syscall_filter)
            }
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(flatten)]
    pub required: Required,
    #[serde(default = "default_duration")]
    pub duration: String,
    pub filter: Option<String>,
}

fn default_duration() -> String {
    "10s".to_string()
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Required {
    #[serde(rename = "probe")]
    ProbeType(String),
    #[serde(rename = "expr")]
    Expr(String),
}

fn parse_duration(duration_str: &str) -> Result<Duration> {
    let duration_str = duration_str.trim();
    let (num_str, unit) = if let Some(stripped) = duration_str.strip_suffix('s') {
        (stripped, "s")
    } else if let Some(stripped) = duration_str.strip_suffix('m') {
        (stripped, "m")
    } else if let Some(stripped) = duration_str.strip_suffix('h') {
        (stripped, "h")
    } else {
        (duration_str, "s")
    };

    let num: u64 = num_str
        .parse()
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid duration: {e}")))?;

    let duration = match unit {
        "s" => Duration::from_secs(num),
        "m" => Duration::from_secs(num * 60),
        "h" => Duration::from_secs(num * 3600),
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Unknown duration unit: {unit}"),
            ));
        }
    };

    Ok(duration)
}

fn parse_events(output: &str) -> Vec<serde_json::Value> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                serde_json::from_str(trimmed).ok()
            } else {
                None
            }
        })
        .collect()
}

fn compute_stats(events: &[serde_json::Value]) -> serde_json::Value {
    let total = events.len();

    let mut by_comm: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for event in events {
        if let Some(comm) = event.get("comm").and_then(|c| c.as_str()) {
            *by_comm.entry(comm.to_string()).or_insert(0) += 1;
        }
    }

    json!({
        "total": total,
        "by_comm": by_comm
    })
}

fn run_trace(program: &str, duration: &Duration) -> Result<(ModuleResult, Option<Value>)> {
    let duration_secs = duration.as_secs();

    let full_program = format!(
        "interval:s:1 {{ if (nsecs / 1000000000 >= {duration_secs}) {{ exit(); }} }} {program}"
    );

    trace!("exec bpftrace with program: {full_program}");

    let output = StdCommand::new("bpftrace")
        .arg("-e")
        .arg(&full_program)
        .arg("-f")
        .arg("json")
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("bpftrace execution failed: {e}"),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() && !stdout.contains("\"comm\"") {
        let error_msg = if !stderr.is_empty() {
            stderr.to_string()
        } else {
            format!("bpftrace exited with status: {}", output.status)
        };
        return Err(Error::new(ErrorKind::SubprocessFail, error_msg));
    }

    let events = parse_events(&stdout);
    let stats = compute_stats(&events);
    let duration_ms = duration.as_millis() as u64;

    let extra = Some(value::to_value(json!({
        "events": events,
        "stats": stats,
        "duration_ms": duration_ms
    }))?);

    let output_msg = format!(
        "Traced for {}ms, captured {} events",
        duration_ms,
        stats.get("total").and_then(|t| t.as_u64()).unwrap_or(0)
    );

    Ok((ModuleResult::new(true, extra, Some(output_msg)), None))
}

#[derive(Debug)]
pub struct Trace;

impl Module for Trace {
    fn get_name(&self) -> &str {
        "trace"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;

        let duration = parse_duration(&params.duration)?;

        let program = match params.required {
            Required::ProbeType(ref probe_str) => {
                let probe: Probe = probe_str.parse().map_err(|e| {
                    Error::new(ErrorKind::InvalidData, format!("Invalid probe: {e}"))
                })?;
                probe.get_bpftrace_program(params.filter.as_deref())
            }
            Required::Expr(ref expr) => expr.clone(),
        };

        run_trace(&program, &duration)
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
    fn test_parse_duration_seconds() {
        let duration = parse_duration("5s").unwrap();
        assert_eq!(duration, Duration::from_secs(5));
    }

    #[test]
    fn test_parse_duration_minutes() {
        let duration = parse_duration("2m").unwrap();
        assert_eq!(duration, Duration::from_secs(120));
    }

    #[test]
    fn test_parse_duration_hours() {
        let duration = parse_duration("1h").unwrap();
        assert_eq!(duration, Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_duration_no_unit() {
        let duration = parse_duration("10").unwrap();
        assert_eq!(duration, Duration::from_secs(10));
    }

    #[test]
    fn test_parse_duration_invalid() {
        let result = parse_duration("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_events() {
        let output = r#"{"comm": "test", "pid": 123, "path": "/etc/hosts"}
some metadata line
{"comm": "cat", "pid": 456, "path": "/etc/passwd"}"#;

        let events = parse_events(output);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["comm"], "test");
        assert_eq!(events[1]["comm"], "cat");
    }

    #[test]
    fn test_compute_stats() {
        let events = vec![
            json!({"comm": "test", "pid": 1}),
            json!({"comm": "test", "pid": 2}),
            json!({"comm": "other", "pid": 3}),
        ];

        let stats = compute_stats(&events);
        assert_eq!(stats["total"], 3);
        assert_eq!(stats["by_comm"]["test"], 2);
        assert_eq!(stats["by_comm"]["other"], 1);
    }

    #[test]
    fn test_probe_bpftrace_programs() {
        let probe = Probe::FileOpens;
        let program = probe.get_bpftrace_program(None);
        assert!(program.contains("sys_enter_openat"));

        let probe = Probe::ProcessExec;
        let program = probe.get_bpftrace_program(None);
        assert!(program.contains("sys_enter_execve"));
    }

    #[test]
    fn test_probe_syscalls_with_filter() {
        let probe = Probe::Syscalls;
        let program = probe.get_bpftrace_program(Some("open,read"));
        assert!(program.contains("syscall == \"open\""));
        assert!(program.contains("syscall == \"read\""));
    }

    #[test]
    fn test_parse_params_probe() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            probe: file_opens
            duration: "5s"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.required,
            Required::ProbeType("file_opens".to_string())
        );
        assert_eq!(params.duration, "5s");
    }

    #[test]
    fn test_parse_params_expr() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            expr: 'tracepoint:syscalls:sys_enter_open { @[comm] = count(); }'
            duration: "10s"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.required,
            Required::Expr("tracepoint:syscalls:sys_enter_open { @[comm] = count(); }".to_string())
        );
    }

    #[test]
    fn test_parse_params_with_filter() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            probe: syscalls
            filter: open,read,write
            duration: "5s"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.filter, Some("open,read,write".to_string()));
    }

    #[test]
    fn test_parse_params_default_duration() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            probe: file_opens
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.duration, "10s");
    }
}
