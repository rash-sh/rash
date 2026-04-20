/// ANCHOR: module
/// # libvirt
///
/// Manage Libvirt virtual machines (domains).
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Define and start a VM
///   libvirt:
///     name: webserver
///     state: running
///     memory: 2048
///     vcpu: 2
///     disk:
///       path: /var/lib/libvirt/images/webserver.qcow2
///       size: 20G
///       format: qcow2
///     network:
///       network_type: bridge
///       source: virbr0
///
/// - name: Define VM with custom XML
///   libvirt:
///     name: myvm
///     state: present
///     xml: |
///       <domain type='kvm'>
///         <name>myvm</name>
///         <memory unit='MiB'>4096</memory>
///         <vcpu>4</vcpu>
///         ...
///       </domain>
///
/// - name: Start a VM
///   libvirt:
///     name: webserver
///     state: running
///
/// - name: Stop a VM
///   libvirt:
///     name: webserver
///     state: stopped
///
/// - name: Pause a VM
///   libvirt:
///     name: webserver
///     state: paused
///
/// - name: Restart a VM
///   libvirt:
///     name: webserver
///     state: restarted
///
/// - name: Set VM autostart
///   libvirt:
///     name: webserver
///     autostart: true
///
/// - name: Undefine a VM
///   libvirt:
///     name: webserver
///     state: undefined
///
/// - name: Undefine a VM with storage
///   libvirt:
///     name: webserver
///     state: undefined
///     remove_storage: true
///
/// - name: Connect to remote libvirt
///   libvirt:
///     name: webserver
///     state: running
///     uri: qemu+ssh://root@192.168.1.10/system
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Running,
    Stopped,
    Paused,
    Undefined,
    Present,
    Destroyed,
    Restarted,
}

fn default_state() -> State {
    State::Present
}

fn default_uri() -> String {
    "qemu:///system".to_string()
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct DiskConfig {
    /// Path to the disk image file.
    path: String,
    /// Size of the disk (e.g., 10G, 20G).
    #[serde(default = "default_disk_size")]
    size: String,
    /// Disk format (qcow2, raw, etc).
    #[serde(default = "default_disk_format")]
    format: String,
    /// Bus type (virtio, ide, scsi, etc).
    #[serde(default = "default_disk_bus")]
    bus: String,
}

fn default_disk_size() -> String {
    "10G".to_string()
}

fn default_disk_format() -> String {
    "qcow2".to_string()
}

fn default_disk_bus() -> String {
    "virtio".to_string()
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct NetworkConfig {
    /// Network interface type (bridge, network, none).
    #[serde(default = "default_network_type")]
    network_type: String,
    /// Source bridge or network name.
    source: Option<String>,
    /// MAC address (auto-generated if not specified).
    mac: Option<String>,
    /// Model type (virtio, e1000, rtl8139, etc).
    #[serde(default = "default_network_model")]
    model: String,
}

fn default_network_type() -> String {
    "network".to_string()
}

fn default_network_model() -> String {
    "virtio".to_string()
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the domain/VM.
    name: String,
    /// State of the domain.
    #[serde(default = "default_state")]
    state: State,
    /// Libvirt connection URI.
    #[serde(default = "default_uri")]
    uri: String,
    /// Domain XML definition (overrides other resource parameters).
    xml: Option<String>,
    /// Memory allocation in MiB.
    memory: Option<u64>,
    /// Number of virtual CPUs.
    vcpu: Option<u32>,
    /// Disk configuration.
    disk: Option<DiskConfig>,
    /// Network interface configuration.
    network: Option<NetworkConfig>,
    /// Set autostart flag on the domain.
    autostart: Option<bool>,
    /// Remove associated storage when undefining.
    #[serde(default)]
    remove_storage: bool,
    /// Force stop (destroy) instead of graceful shutdown.
    #[serde(default)]
    force: bool,
    /// Enable or disable the domain (alias for state management).
    enabled: Option<bool>,
}

#[derive(Debug)]
pub struct Libvirt;

impl Module for Libvirt {
    fn get_name(&self) -> &str {
        "libvirt"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((libvirt(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct LibvirtClient {
    uri: String,
    check_mode: bool,
}

impl LibvirtClient {
    fn new(uri: String, check_mode: bool) -> Self {
        LibvirtClient { uri, check_mode }
    }

    fn exec_virsh(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let mut full_args: Vec<String> = vec!["--connect".to_string(), self.uri.clone()];
        for arg in args {
            full_args.push(arg.to_string());
        }

        let output = Command::new("virsh")
            .args(&full_args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `virsh {:?}`", full_args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing virsh: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn domain_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_virsh(&["dominfo", name], false)?;
        Ok(output.status.success())
    }

    fn get_domain_state(&self, name: &str) -> Result<String> {
        let output = self.exec_virsh(&["domstate", name], true)?;
        let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(state)
    }

    fn is_running(&self, name: &str) -> Result<bool> {
        let state = self.get_domain_state(name)?;
        Ok(state == "running")
    }

    fn is_paused(&self, name: &str) -> Result<bool> {
        let state = self.get_domain_state(name)?;
        Ok(state == "paused" || state == "pmsuspended")
    }

    fn is_shut_off(&self, name: &str) -> Result<bool> {
        let state = self.get_domain_state(name)?;
        Ok(state == "shut off")
    }

    fn get_autostart(&self, name: &str) -> Result<bool> {
        let output = self.exec_virsh(&["dominfo", name], true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("Autostart:") {
                return Ok(line.contains("enable"));
            }
        }
        Ok(false)
    }

    fn define_domain(&self, xml: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        use std::io::Write;
        let mut child = Command::new("virsh")
            .arg("--connect")
            .arg(&self.uri)
            .arg("define")
            .arg("/dev/stdin")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(xml.as_bytes())
                .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error defining domain: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(true)
    }

    fn start_domain(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        if self.is_running(name)? {
            return Ok(false);
        }
        self.exec_virsh(&["start", name], true)?;
        Ok(true)
    }

    fn stop_domain(&self, name: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        if self.is_shut_off(name)? {
            return Ok(false);
        }
        if force {
            self.exec_virsh(&["destroy", name], true)?;
        } else {
            self.exec_virsh(&["shutdown", name], true)?;
        }
        Ok(true)
    }

    fn pause_domain(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        if self.is_paused(name)? {
            return Ok(false);
        }
        if !self.is_running(name)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Domain '{}' must be running to pause", name),
            ));
        }
        self.exec_virsh(&["suspend", name], true)?;
        Ok(true)
    }

    fn resume_domain(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        if self.is_running(name)? {
            return Ok(false);
        }
        if !self.is_paused(name)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Domain '{}' is not paused", name),
            ));
        }
        self.exec_virsh(&["resume", name], true)?;
        Ok(true)
    }

    fn reboot_domain(&self, name: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        if force {
            self.exec_virsh(&["reset", name], true)?;
        } else {
            self.exec_virsh(&["reboot", name], true)?;
        }
        Ok(true)
    }

    fn undefine_domain(&self, name: &str, remove_storage: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        if !self.domain_exists(name)? {
            return Ok(false);
        }
        let mut args: Vec<&str> = vec!["undefine", name];
        if remove_storage {
            args.push("--remove-all-storage");
        }
        self.exec_virsh(&args, true)?;
        Ok(true)
    }

    fn set_autostart(&self, name: &str, enabled: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }
        let current = self.get_autostart(name)?;
        if current == enabled {
            return Ok(false);
        }
        if enabled {
            self.exec_virsh(&["autostart", name], true)?;
        } else {
            self.exec_virsh(&["autostart", "--disable", name], true)?;
        }
        Ok(true)
    }

    fn get_domain_info(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if self.domain_exists(name)? {
            let state = self.get_domain_state(name)?;
            let autostart = self.get_autostart(name)?;
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert(
                "name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
            result.insert(
                "state".to_string(),
                serde_json::Value::String(state.clone()),
            );
            result.insert(
                "running".to_string(),
                serde_json::Value::Bool(state == "running"),
            );
            result.insert(
                "paused".to_string(),
                serde_json::Value::Bool(state == "paused"),
            );
            result.insert("autostart".to_string(), serde_json::Value::Bool(autostart));
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn generate_domain_xml(params: &Params) -> String {
    if let Some(ref xml) = params.xml {
        return xml.clone();
    }

    let memory = params.memory.unwrap_or(1024);
    let vcpu = params.vcpu.unwrap_or(1);
    let name = &params.name;

    let disk_xml = if let Some(ref disk) = params.disk {
        format!(
            r#"    <disk type='file' device='disk'>
      <driver name='qemu' type='{}'/>
      <source file='{}'/>
      <target dev='vda' bus='{}'/>
    </disk>"#,
            disk.format, disk.path, disk.bus
        )
    } else {
        String::new()
    };

    let network_xml = if let Some(ref net) = params.network {
        let source_xml = match net.network_type.as_str() {
            "bridge" => {
                if let Some(ref br) = net.source {
                    format!("<source bridge='{}'/>", br)
                } else {
                    String::new()
                }
            }
            "network" => {
                if let Some(ref net_name) = net.source {
                    format!("<source network='{}'/>", net_name)
                } else {
                    "<source network='default'/>".to_string()
                }
            }
            _ => String::new(),
        };
        let mac_xml = net
            .mac
            .as_ref()
            .map(|m| format!("<mac address='{}'/>", m))
            .unwrap_or_default();
        format!(
            r#"    <interface type='{}'>
      {}
      {}
      <model type='{}'/>
    </interface>"#,
            net.network_type, source_xml, mac_xml, net.model
        )
    } else {
        String::new()
    };

    format!(
        r#"<domain type='kvm'>
  <name>{name}</name>
  <memory unit='MiB'>{memory}</memory>
  <vcpu>{vcpu}</vcpu>
  <os>
    <type arch='x86_64'>hvm</type>
  </os>
  <devices>
{disk_xml}
{network_xml}
    <console type='pty'>
      <target type='serial' port='0'/>
    </console>
    <graphics type='vnc' port='-1' autoport='yes'/>
  </devices>
</domain>"#
    )
}

fn validate_domain_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Domain name cannot be empty",
        ));
    }
    if name.len() > 255 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Domain name too long (max 255 characters)",
        ));
    }
    Ok(())
}

fn libvirt(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_domain_name(&params.name)?;

    let client = LibvirtClient::new(params.uri.clone(), check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Running => {
            let exists = client.domain_exists(&params.name)?;
            if !exists {
                let xml = generate_domain_xml(&params);
                client.define_domain(&xml)?;
                diff("state: undefined".to_string(), "state: defined".to_string());
                output_messages.push(format!("Domain '{}' defined", params.name));
                changed = true;
            }

            let was_paused = exists && client.is_paused(&params.name)?;

            if was_paused {
                client.resume_domain(&params.name)?;
                diff("state: paused".to_string(), "state: running".to_string());
                output_messages.push(format!("Domain '{}' resumed", params.name));
                changed = true;
            } else if client.start_domain(&params.name)? {
                diff("state: stopped".to_string(), "state: running".to_string());
                output_messages.push(format!("Domain '{}' started", params.name));
                changed = true;
            } else if !changed {
                output_messages.push(format!("Domain '{}' already running", params.name));
            }
        }
        State::Present | State::Destroyed => {
            let exists = client.domain_exists(&params.name)?;
            if !exists {
                let xml = generate_domain_xml(&params);
                client.define_domain(&xml)?;
                diff("state: undefined".to_string(), "state: defined".to_string());
                output_messages.push(format!("Domain '{}' defined", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Domain '{}' already defined", params.name));
            }

            if params.state == State::Destroyed && client.is_running(&params.name)? {
                client.stop_domain(&params.name, params.force)?;
                diff("state: running".to_string(), "state: destroyed".to_string());
                output_messages.push(format!("Domain '{}' destroyed", params.name));
                changed = true;
            }
        }
        State::Stopped => {
            if !client.domain_exists(&params.name)? {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Domain '{}' does not exist", params.name),
                ));
            }
            if client.stop_domain(&params.name, params.force)? {
                diff("state: running".to_string(), "state: stopped".to_string());
                output_messages.push(format!("Domain '{}' stopped", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Domain '{}' already stopped", params.name));
            }
        }
        State::Paused => {
            if !client.domain_exists(&params.name)? {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Domain '{}' does not exist", params.name),
                ));
            }
            if client.pause_domain(&params.name)? {
                diff("state: running".to_string(), "state: paused".to_string());
                output_messages.push(format!("Domain '{}' paused", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Domain '{}' already paused", params.name));
            }
        }
        State::Restarted => {
            if !client.domain_exists(&params.name)? {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Domain '{}' does not exist", params.name),
                ));
            }
            client.reboot_domain(&params.name, params.force)?;
            output_messages.push(format!("Domain '{}' rebooted", params.name));
            changed = true;
        }
        State::Undefined => {
            if client.undefine_domain(&params.name, params.remove_storage)? {
                diff("state: defined".to_string(), "state: undefined".to_string());
                if params.remove_storage {
                    output_messages.push(format!(
                        "Domain '{}' undefined with storage removed",
                        params.name
                    ));
                } else {
                    output_messages.push(format!("Domain '{}' undefined", params.name));
                }
                changed = true;
            } else {
                output_messages.push(format!("Domain '{}' already undefined", params.name));
            }
        }
    }

    if let Some(autostart) = params.autostart
        && (client.domain_exists(&params.name)? || check_mode)
        && client.set_autostart(&params.name, autostart)?
    {
        diff(
            format!("autostart: {}", !autostart),
            format!("autostart: {}", autostart),
        );
        output_messages.push(format!(
            "Domain '{}' autostart set to {}",
            params.name, autostart
        ));
        changed = true;
    }

    let extra = client.get_domain_info(&params.name)?;

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult {
        changed,
        output: final_output,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webserver");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.uri, "qemu:///system");
    }

    #[test]
    fn test_parse_params_with_state_running() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            state: running
            memory: 2048
            vcpu: 2
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webserver");
        assert_eq!(params.state, State::Running);
        assert_eq!(params.memory, Some(2048));
        assert_eq!(params.vcpu, Some(2));
    }

    #[test]
    fn test_parse_params_with_disk() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            state: running
            disk:
              path: /var/lib/libvirt/images/myvm.qcow2
              size: 20G
              format: qcow2
              bus: virtio
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let disk = params.disk.unwrap();
        assert_eq!(disk.path, "/var/lib/libvirt/images/myvm.qcow2");
        assert_eq!(disk.size, "20G");
        assert_eq!(disk.format, "qcow2");
        assert_eq!(disk.bus, "virtio");
    }

    #[test]
    fn test_parse_params_with_network_bridge() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            network:
              network_type: bridge
              source: virbr0
              model: virtio
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let net = params.network.unwrap();
        assert_eq!(net.network_type, "bridge");
        assert_eq!(net.source, Some("virbr0".to_string()));
        assert_eq!(net.model, "virtio");
    }

    #[test]
    fn test_parse_params_with_network_default() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            network:
              source: default
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let net = params.network.unwrap();
        assert_eq!(net.network_type, "network");
        assert_eq!(net.source, Some("default".to_string()));
    }

    #[test]
    fn test_parse_params_all_states() {
        for state_str in &[
            "running",
            "stopped",
            "paused",
            "undefined",
            "present",
            "destroyed",
            "restarted",
        ] {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                name: test
                state: {}
                "#,
                state_str
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(params.name, "test");
        }
    }

    #[test]
    fn test_parse_params_autostart() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            autostart: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.autostart, Some(true));
    }

    #[test]
    fn test_parse_params_uri() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            uri: qemu+ssh://root@192.168.1.10/system
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.uri, "qemu+ssh://root@192.168.1.10/system");
    }

    #[test]
    fn test_parse_params_force_and_storage() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            state: undefined
            force: true
            remove_storage: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
        assert!(params.remove_storage);
    }

    #[test]
    fn test_parse_params_xml() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            xml: |
              <domain type='kvm'>
                <name>myvm</name>
                <memory unit='MiB'>4096</memory>
              </domain>
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.xml.is_some());
        assert!(params.xml.unwrap().contains("<domain type='kvm'>"));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_domain_name() {
        assert!(validate_domain_name("webserver").is_ok());
        assert!(validate_domain_name("web-server").is_ok());
        assert!(validate_domain_name("web_server").is_ok());
        assert!(validate_domain_name("webserver.example.com").is_ok());

        assert!(validate_domain_name("").is_err());
        assert!(validate_domain_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn test_generate_domain_xml_minimal() {
        let params = Params {
            name: "testvm".to_string(),
            state: State::Running,
            uri: "qemu:///system".to_string(),
            xml: None,
            memory: Some(2048),
            vcpu: Some(2),
            disk: None,
            network: None,
            autostart: None,
            remove_storage: false,
            force: false,
            enabled: None,
        };
        let xml = generate_domain_xml(&params);
        assert!(xml.contains("<name>testvm</name>"));
        assert!(xml.contains("<memory unit='MiB'>2048</memory>"));
        assert!(xml.contains("<vcpu>2</vcpu>"));
        assert!(xml.contains("<domain type='kvm'>"));
    }

    #[test]
    fn test_generate_domain_xml_with_disk() {
        let params = Params {
            name: "testvm".to_string(),
            state: State::Running,
            uri: "qemu:///system".to_string(),
            xml: None,
            memory: Some(1024),
            vcpu: Some(1),
            disk: Some(DiskConfig {
                path: "/tmp/test.qcow2".to_string(),
                size: "10G".to_string(),
                format: "qcow2".to_string(),
                bus: "virtio".to_string(),
            }),
            network: None,
            autostart: None,
            remove_storage: false,
            force: false,
            enabled: None,
        };
        let xml = generate_domain_xml(&params);
        assert!(xml.contains("<source file='/tmp/test.qcow2'/>"));
        assert!(xml.contains("type='qcow2'"));
    }

    #[test]
    fn test_generate_domain_xml_with_network_bridge() {
        let params = Params {
            name: "testvm".to_string(),
            state: State::Running,
            uri: "qemu:///system".to_string(),
            xml: None,
            memory: Some(1024),
            vcpu: Some(1),
            disk: None,
            network: Some(NetworkConfig {
                network_type: "bridge".to_string(),
                source: Some("virbr0".to_string()),
                mac: Some("52:54:00:12:34:56".to_string()),
                model: "virtio".to_string(),
            }),
            autostart: None,
            remove_storage: false,
            force: false,
            enabled: None,
        };
        let xml = generate_domain_xml(&params);
        assert!(xml.contains("<source bridge='virbr0'/>"));
        assert!(xml.contains("<mac address='52:54:00:12:34:56'/>"));
    }

    #[test]
    fn test_generate_domain_xml_custom() {
        let custom_xml = "<domain type='kvm'><name>custom</name></domain>".to_string();
        let params = Params {
            name: "testvm".to_string(),
            state: State::Running,
            uri: "qemu:///system".to_string(),
            xml: Some(custom_xml.clone()),
            memory: None,
            vcpu: None,
            disk: None,
            network: None,
            autostart: None,
            remove_storage: false,
            force: false,
            enabled: None,
        };
        let xml = generate_domain_xml(&params);
        assert_eq!(xml, custom_xml);
    }

    #[test]
    fn test_disk_config_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test.qcow2
            "#,
        )
        .unwrap();
        let disk: DiskConfig = serde_norway::from_value(yaml).unwrap();
        assert_eq!(disk.path, "/tmp/test.qcow2");
        assert_eq!(disk.size, "10G");
        assert_eq!(disk.format, "qcow2");
        assert_eq!(disk.bus, "virtio");
    }

    #[test]
    fn test_network_config_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: default
            "#,
        )
        .unwrap();
        let net: NetworkConfig = serde_norway::from_value(yaml).unwrap();
        assert_eq!(net.network_type, "network");
        assert_eq!(net.source, Some("default".to_string()));
        assert_eq!(net.model, "virtio");
        assert_eq!(net.mac, None);
    }

    #[test]
    fn test_generate_domain_xml_with_network_network() {
        let params = Params {
            name: "testvm".to_string(),
            state: State::Running,
            uri: "qemu:///system".to_string(),
            xml: None,
            memory: Some(1024),
            vcpu: Some(1),
            disk: None,
            network: Some(NetworkConfig {
                network_type: "network".to_string(),
                source: Some("default".to_string()),
                mac: None,
                model: "virtio".to_string(),
            }),
            autostart: None,
            remove_storage: false,
            force: false,
            enabled: None,
        };
        let xml = generate_domain_xml(&params);
        assert!(xml.contains("<source network='default'/>"));
    }

    #[test]
    fn test_generate_domain_xml_network_no_source() {
        let params = Params {
            name: "testvm".to_string(),
            state: State::Running,
            uri: "qemu:///system".to_string(),
            xml: None,
            memory: Some(1024),
            vcpu: Some(1),
            disk: None,
            network: Some(NetworkConfig {
                network_type: "network".to_string(),
                source: None,
                mac: None,
                model: "virtio".to_string(),
            }),
            autostart: None,
            remove_storage: false,
            force: false,
            enabled: None,
        };
        let xml = generate_domain_xml(&params);
        assert!(xml.contains("<source network='default'/>"));
    }
}
