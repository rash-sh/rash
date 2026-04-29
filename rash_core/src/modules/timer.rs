/// ANCHOR: module
/// # timer
///
/// Time task execution for debugging and performance profiling.
///
/// This module provides named timers that can be started, stopped, and read
/// to measure elapsed time between tasks. Useful for debugging, performance
/// optimization, and IoT devices with limited resources.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: parameters
/// | Parameter | Required | Type   | Values          | Description                                  |
/// |-----------|----------|--------|-----------------|----------------------------------------------|
/// | name      | true     | string |                 | Timer name                                   |
/// | state     | false    | string | started/stopped/read | Timer action (default: started)         |
/// | precision | false    | string | ms/us/ns        | Timer precision (default: ms)                |
///
/// ANCHOR_END: parameters
/// ANCHOR: examples
/// ## Examples
///
/// ### Basic timing
///
/// ```yaml
/// - name: Start performance timer
///   timer:
///     name: app_startup
///     state: started
///
/// - name: Run application startup
///   command: ./startup.sh
///
/// - name: Stop timer and get elapsed time
///   timer:
///     name: app_startup
///     state: stopped
///   register: elapsed
///
/// - name: Log startup time
///   debug:
///     msg: "Startup took {{ elapsed.extra.elapsed_ms }} milliseconds"
/// ```
///
/// ### Read without stopping
///
/// ```yaml
/// - timer:
///     name: long_operation
///     state: started
///
/// - command: ./step1.sh
///
/// - timer:
///     name: long_operation
///     state: read
///   register: checkpoint
///
/// - debug:
///     msg: "Checkpoint: {{ checkpoint.extra.elapsed_ms }}ms elapsed"
///
/// - command: ./step2.sh
///
/// - timer:
///     name: long_operation
///     state: stopped
///   register: final_time
/// ```
///
/// ### Multiple timers with different precision
///
/// ```yaml
/// - timer:
///     name: fast_op
///     state: started
///     precision: us
///
/// - command: ./fast.sh
///
/// - timer:
///     name: fast_op
///     state: stopped
///   register: fast_elapsed
///
/// - debug:
///     msg: "Fast op took {{ fast_elapsed.extra.elapsed_us }} microseconds"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use std::time::Instant;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use strum_macros::{Display, EnumString};

static TIMERS: LazyLock<RwLock<HashMap<String, Instant>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Clone, Debug, Default, PartialEq, Deserialize, EnumString, Display)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
pub enum State {
    #[default]
    Started,
    Stopped,
    Read,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, EnumString, Display)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
pub enum Precision {
    #[default]
    Ms,
    Us,
    Ns,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    name: String,
    #[serde(default)]
    state: State,
    #[serde(default)]
    precision: Precision,
}

fn start_timer(name: &str) -> Result<ModuleResult> {
    let mut timers = TIMERS.write().map_err(|e| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to acquire timer lock: {e}"),
        )
    })?;
    timers.insert(name.to_string(), Instant::now());

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Timer '{name}' started")),
    ))
}

fn stop_timer(name: &str, precision: &Precision) -> Result<ModuleResult> {
    let mut timers = TIMERS.write().map_err(|e| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to acquire timer lock: {e}"),
        )
    })?;

    let start = timers.remove(name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Timer '{name}' not found. Start it first."),
        )
    })?;

    let elapsed = start.elapsed();
    let (elapsed_ms, elapsed_us, elapsed_ns) = compute_elapsed(elapsed);

    let extra = Some(value::to_value(json!({
        "elapsed_ms": elapsed_ms,
        "elapsed_us": elapsed_us,
        "elapsed_ns": elapsed_ns,
    }))?);

    let output = format_elapsed(name, elapsed_ms, elapsed_us, elapsed_ns, precision);

    Ok(ModuleResult::new(true, extra, Some(output)))
}

fn read_timer(name: &str, precision: &Precision) -> Result<ModuleResult> {
    let timers = TIMERS.read().map_err(|e| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to acquire timer lock: {e}"),
        )
    })?;

    let start = timers.get(name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Timer '{name}' not found. Start it first."),
        )
    })?;

    let elapsed = start.elapsed();
    let (elapsed_ms, elapsed_us, elapsed_ns) = compute_elapsed(elapsed);

    let extra = Some(value::to_value(json!({
        "elapsed_ms": elapsed_ms,
        "elapsed_us": elapsed_us,
        "elapsed_ns": elapsed_ns,
    }))?);

    let output = format_elapsed(name, elapsed_ms, elapsed_us, elapsed_ns, precision);

    Ok(ModuleResult::new(false, extra, Some(output)))
}

fn compute_elapsed(elapsed: std::time::Duration) -> (u64, u64, u64) {
    let elapsed_ms = elapsed.as_millis() as u64;
    let elapsed_us = elapsed.as_micros() as u64;
    let elapsed_ns = elapsed.as_nanos() as u64;
    (elapsed_ms, elapsed_us, elapsed_ns)
}

fn format_elapsed(
    name: &str,
    elapsed_ms: u64,
    elapsed_us: u64,
    elapsed_ns: u64,
    precision: &Precision,
) -> String {
    match precision {
        Precision::Ms => format!("Timer '{name}': {elapsed_ms}ms"),
        Precision::Us => format!("Timer '{name}': {elapsed_us}us"),
        Precision::Ns => format!("Timer '{name}': {elapsed_ns}ns"),
    }
}

pub fn timer(params: Params) -> Result<ModuleResult> {
    match params.state {
        State::Started => start_timer(&params.name),
        State::Stopped => stop_timer(&params.name, &params.precision),
        State::Read => read_timer(&params.name, &params.precision),
    }
}

#[derive(Debug)]
pub struct Timer;

impl Module for Timer {
    fn get_name(&self) -> &str {
        "timer"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((timer(parse_params(optional_params)?)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_parse_params_started() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test_timer
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "test_timer");
        assert_eq!(params.state, State::Started);
        assert_eq!(params.precision, Precision::Ms);
    }

    #[test]
    fn test_parse_params_stopped() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test_timer
            state: stopped
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Stopped);
    }

    #[test]
    fn test_parse_params_read() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test_timer
            state: read
            precision: us
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Read);
        assert_eq!(params.precision, Precision::Us);
    }

    #[test]
    fn test_parse_params_ns_precision() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test_timer
            state: stopped
            precision: ns
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.precision, Precision::Ns);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test_timer
            invalid: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_start_timer() {
        let result = start_timer("test_start").unwrap();
        assert!(result.get_changed());
        assert_eq!(
            result.get_output(),
            Some("Timer 'test_start' started".to_string())
        );
    }

    #[test]
    fn test_stop_timer_not_found() {
        let result = stop_timer("nonexistent", &Precision::Ms);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_read_timer_not_found() {
        let result = read_timer("nonexistent", &Precision::Ms);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_start_stop_timer() {
        let unique_name = format!("test_stop_{}", std::process::id());
        start_timer(&unique_name).unwrap();
        thread::sleep(Duration::from_millis(10));
        let result = stop_timer(&unique_name, &Precision::Ms).unwrap();
        assert!(result.get_changed());
        let extra = result.get_extra().unwrap();
        let elapsed_ms = extra.get("elapsed_ms").unwrap().as_u64().unwrap();
        assert!(elapsed_ms >= 10);
    }

    #[test]
    fn test_start_read_timer() {
        let unique_name = format!("test_read_{}", std::process::id());
        start_timer(&unique_name).unwrap();
        thread::sleep(Duration::from_millis(10));
        let result = read_timer(&unique_name, &Precision::Ms).unwrap();
        assert!(!result.get_changed());
        let extra = result.get_extra().unwrap();
        let elapsed_ms = extra.get("elapsed_ms").unwrap().as_u64().unwrap();
        assert!(elapsed_ms >= 10);
    }

    #[test]
    fn test_timer_full_flow() {
        let unique_name = format!("test_flow_{}", std::process::id());
        let result = timer(Params {
            name: unique_name.clone(),
            state: State::Started,
            precision: Precision::Ms,
        })
        .unwrap();
        assert!(result.get_changed());

        thread::sleep(Duration::from_millis(5));

        let result = timer(Params {
            name: unique_name.clone(),
            state: State::Read,
            precision: Precision::Us,
        })
        .unwrap();
        assert!(!result.get_changed());
        let extra = result.get_extra().unwrap();
        let elapsed_us = extra.get("elapsed_us").unwrap().as_u64().unwrap();
        assert!(elapsed_us >= 5000);

        thread::sleep(Duration::from_millis(5));

        let result = timer(Params {
            name: unique_name,
            state: State::Stopped,
            precision: Precision::Ns,
        })
        .unwrap();
        assert!(result.get_changed());
        let extra = result.get_extra().unwrap();
        let elapsed_ns = extra.get("elapsed_ns").unwrap().as_u64().unwrap();
        assert!(elapsed_ns >= 10_000_000);
    }

    #[test]
    fn test_timer_module_exec() {
        let timer_module = Timer;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: exec_test
            state: started
            "#,
        )
        .unwrap();
        let (result, _) = timer_module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        assert!(result.get_changed());
    }
}
