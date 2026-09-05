use crate::error::{Error, ErrorKind, Result};
use crate::process::{OutputMode, ProcessSpec};

use serde_norway::Value as YamlValue;

fn string_field(params: &YamlValue, name: &str) -> Result<Option<String>> {
    match params.get(name) {
        Some(YamlValue::Null) | None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("{name} must be a string"))),
    }
}

fn output_mode(params: &YamlValue, name: &str) -> Result<OutputMode> {
    match params.get(name) {
        Some(value) => serde_norway::from_value(value.clone())
            .map_err(|e| Error::new(ErrorKind::InvalidData, e)),
        None => Ok(OutputMode::Capture),
    }
}

fn apply_common(spec: &mut ProcessSpec, params: &YamlValue) -> Result<()> {
    spec.chdir = string_field(params, "chdir")?;
    spec.stdin = string_field(params, "stdin")?;
    spec.stdout = output_mode(params, "stdout")?;
    spec.stderr = output_mode(params, "stderr")?;
    Ok(())
}

fn command_spec(params: &YamlValue) -> Result<ProcessSpec> {
    if let Some(command) = params.as_str() {
        return Ok(ProcessSpec::shell(command, "/bin/sh"));
    }

    if params
        .get("transfer_pid")
        .and_then(YamlValue::as_bool)
        .unwrap_or(false)
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "transfer_pid cannot be combined with async execution",
        ));
    }

    let mut spec = if let Some(command) = string_field(params, "cmd")? {
        ProcessSpec::shell(command, "/bin/sh")
    } else if let Some(argv) = params.get("argv").and_then(YamlValue::as_sequence) {
        let argv: Vec<String> = argv
            .iter()
            .map(|value| {
                value.as_str().map(String::from).ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "command argv entries must be strings",
                    )
                })
            })
            .collect::<Result<_>>()?;
        let program = argv
            .first()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "command argv cannot be empty"))?;
        let mut spec = ProcessSpec::new(program);
        spec.args.extend(argv.iter().skip(1).cloned());
        spec
    } else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "command requires cmd or argv",
        ));
    };
    apply_common(&mut spec, params)?;
    Ok(spec)
}

fn shell_spec(params: &YamlValue) -> Result<ProcessSpec> {
    if let Some(command) = params.as_str() {
        return Ok(ProcessSpec::shell(command, "/bin/sh"));
    }

    let command = string_field(params, "cmd")?
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "shell requires cmd"))?;
    let executable = string_field(params, "executable")?.unwrap_or_else(|| "/bin/sh".to_owned());
    let mut spec = ProcessSpec::shell(command, executable);
    apply_common(&mut spec, params)?;
    Ok(spec)
}

pub(super) fn from_module(module_name: &str, params: &YamlValue) -> Result<ProcessSpec> {
    match module_name {
        "command" => command_spec(params),
        "shell" => shell_spec(params),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Async execution only supports command/shell, got: {module_name}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_argv_is_direct() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            argv: [printf, "%s", "hello world"]
            stdout: tee
            "#,
        )
        .unwrap();
        let spec = from_module("command", &yaml).unwrap();
        assert_eq!(spec.program, "printf");
        assert_eq!(spec.args, vec!["%s", "hello world"]);
        assert_eq!(spec.stdout, OutputMode::Tee);
    }

    #[test]
    fn shell_honors_executable() {
        let yaml: YamlValue =
            serde_norway::from_str("cmd: echo hi\nexecutable: /bin/bash").unwrap();
        let spec = from_module("shell", &yaml).unwrap();
        assert_eq!(spec.program, "/bin/bash");
        assert_eq!(spec.args[0], "-c");
    }

    #[test]
    fn async_transfer_pid_is_rejected() {
        let yaml: YamlValue = serde_norway::from_str("cmd: echo hi\ntransfer_pid: true").unwrap();
        assert!(from_module("command", &yaml).is_err());
    }
}
