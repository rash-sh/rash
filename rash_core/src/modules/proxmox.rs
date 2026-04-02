/// ANCHOR: module
/// # proxmox
///
/// Manage Proxmox VE virtual machines and containers.
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
/// - name: Create a VM
///   proxmox:
///     name: myvm
///     node: pve1
///     type: qemu
///     state: present
///     cores: 2
///     memory: 2048
///     disk: "local-lvm:10"
///     net: "virtio,bridge=vmbr0"
///
/// - name: Create a container
///   proxmox:
///     name: myct
///     node: pve1
///     type: lxc
///     state: present
///     cores: 1
///     memory: 512
///     disk: "local-lvm:4"
///     template: "local:vztmpl/alpine-3.18.tar.gz"
///     net: "name=eth0,bridge=vmbr0,ip=dhcp"
///
/// - name: Start a VM
///   proxmox:
///     name: myvm
///     node: pve1
///     state: started
///
/// - name: Stop a VM
///   proxmox:
///     name: myvm
///     node: pve1
///     state: stopped
///
/// - name: Restart a VM
///   proxmox:
///     name: myvm
///     node: pve1
///     state: restarted
///
/// - name: Remove a VM
///   proxmox:
///     name: myvm
///     node: pve1
///     state: absent
///
/// - name: Create VM with specific VMID
///   proxmox:
///     vmid: 100
///     node: pve1
///     type: qemu
///     state: present
///     cores: 4
///     memory: 4096
///     disk: "local-lvm:20"
///
/// - name: Clone a VM
///   proxmox:
///     name: cloned_vm
///     node: pve1
///     type: qemu
///     state: cloned
///     source: template_vm
///
/// - name: Create container with password
///   proxmox:
///     name: myct
///     node: pve1
///     type: lxc
///     state: present
///     password: "{{ container_password }}"
///     unprivileged: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum VmType {
    Qemu,
    Lxc,
}

impl std::fmt::Display for VmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmType::Qemu => write!(f, "qemu"),
            VmType::Lxc => write!(f, "lxc"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Present,
    Started,
    Stopped,
    Restarted,
    Cloned,
}

fn default_state() -> State {
    State::Present
}

fn default_cores() -> u32 {
    1
}

fn default_memory() -> u32 {
    512
}

fn default_onboot() -> bool {
    false
}

fn default_unprivileged() -> bool {
    false
}

fn default_force() -> bool {
    false
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the VM or container.
    pub name: Option<String>,
    /// Unique VM/Container ID (vmid). If not provided, next available ID is used.
    pub vmid: Option<u32>,
    /// Proxmox node name.
    pub node: String,
    /// Type of resource: qemu (VM) or lxc (container).
    #[serde(default = "default_type", alias = "type")]
    pub vm_type: VmType,
    /// Desired state.
    #[serde(default = "default_state")]
    pub state: State,
    /// Number of CPU cores.
    #[serde(default = "default_cores")]
    pub cores: u32,
    /// Memory size in MB.
    #[serde(default = "default_memory")]
    pub memory: u32,
    /// Disk configuration (e.g., "local-lvm:10").
    pub disk: Option<String>,
    /// Network configuration.
    pub net: Option<String>,
    /// Template for container creation (LXC only).
    pub template: Option<String>,
    /// Root password for container (LXC only).
    pub password: Option<String>,
    /// OS type for VM (e.g., "l26" for Linux 2.6+).
    pub ostype: Option<String>,
    /// Boot automatically on node startup.
    #[serde(default = "default_onboot")]
    pub onboot: bool,
    /// Create unprivileged container (LXC only).
    #[serde(default = "default_unprivileged")]
    pub unprivileged: bool,
    /// Description/comment.
    pub description: Option<String>,
    /// Source VM/template name for cloning.
    pub source: Option<String>,
    /// Force operations (stop/remove even if running).
    #[serde(default = "default_force")]
    pub force: bool,
    /// Timeout for stop operations in seconds.
    #[serde(default = "default_timeout")]
    pub timeout: u32,
    /// CPU units (relative weight).
    pub cpuunits: Option<u32>,
    /// CPU limit (number of CPUs, 0 means unlimited).
    pub cpulimit: Option<u32>,
    /// Swap space in MB (LXC only).
    pub swap: Option<u32>,
    /// Hostname for container (LXC only).
    pub hostname: Option<String>,
    /// Pool name.
    pub pool: Option<String>,
    /// Storage for disk.
    pub storage: Option<String>,
}

fn default_type() -> VmType {
    VmType::Qemu
}

fn default_timeout() -> u32 {
    60
}

#[derive(Debug)]
pub struct Proxmox;

struct ProxmoxClient {
    check_mode: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VmInfo {
    vmid: u32,
    name: String,
    status: String,
    node: String,
    vm_type: VmType,
}

impl ProxmoxClient {
    fn new(check_mode: bool) -> Self {
        ProxmoxClient { check_mode }
    }

    fn exec_pvesh(&self, args: &[&str], check_success: bool) -> Result<Output> {
        if self.check_mode {
            trace!("check_mode: skipping pvesh {:?}", args);
            return Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }

        let output = Command::new("pvesh")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        trace!("command: `pvesh {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing pvesh: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn get_next_vmid(&self) -> Result<u32> {
        let output = self.exec_pvesh(&["get", "/cluster/nextid"], true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.trim().parse::<u32>().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse next vmid: {e}"),
            )
        })
    }

    fn find_vm_by_name(&self, name: &str) -> Result<Option<VmInfo>> {
        let output = self.exec_pvesh(&["get", "/cluster/resources", "--type", "vm"], false)?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let vmid_part = parts[0];
                let type_part = parts[1];
                let name_part = parts[2].trim_start_matches('"').trim_end_matches('"');
                let node_part = parts[3];
                let status_part = if parts.len() >= 5 {
                    parts[4].trim_start_matches('"').trim_end_matches('"')
                } else {
                    "unknown"
                };

                if name_part == name {
                    let vmid = vmid_part.parse::<u32>().map_err(|e| {
                        Error::new(ErrorKind::InvalidData, format!("Failed to parse vmid: {e}"))
                    })?;
                    let vm_type = match type_part {
                        "lxc" => VmType::Lxc,
                        "qemu" => VmType::Qemu,
                        _ => continue,
                    };
                    return Ok(Some(VmInfo {
                        vmid,
                        name: name_part.to_string(),
                        status: status_part.to_string(),
                        node: node_part.to_string(),
                        vm_type,
                    }));
                }
            }
        }
        Ok(None)
    }

    fn get_vm_status(&self, node: &str, vmid: u32, vm_type: &VmType) -> Result<String> {
        let path = match vm_type {
            VmType::Qemu => format!("/nodes/{node}/qemu/{vmid}/status/current"),
            VmType::Lxc => format!("/nodes/{node}/lxc/{vmid}/status/current"),
        };

        let output = self.exec_pvesh(&["get", &path], false)?;

        if !output.status.success() {
            return Ok("unknown".to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("status:") || line.contains("status") {
                let status = line.split(':').next_back().unwrap_or("unknown").trim();
                return Ok(status.to_string());
            }
        }
        Ok("unknown".to_string())
    }

    fn vm_exists(&self, node: &str, vmid: u32, vm_type: &VmType) -> Result<bool> {
        let path = match vm_type {
            VmType::Qemu => format!("/nodes/{node}/qemu/{vmid}/status/current"),
            VmType::Lxc => format!("/nodes/{node}/lxc/{vmid}/status/current"),
        };

        let output = self.exec_pvesh(&["get", &path], false)?;
        Ok(output.status.success())
    }

    fn is_running(&self, node: &str, vmid: u32, vm_type: &VmType) -> Result<bool> {
        let status = self.get_vm_status(node, vmid, vm_type)?;
        Ok(status == "running")
    }

    fn create_vm(&self, params: &Params, vmid: u32) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let path = match params.vm_type {
            VmType::Qemu => format!("/nodes/{}/qemu", params.node),
            VmType::Lxc => format!("/nodes/{}/lxc", params.node),
        };

        let mut args: Vec<String> = vec!["create".to_string(), path.clone()];
        args.push(format!("vmid={}", vmid));
        args.push(format!("cores={}", params.cores));
        args.push(format!("memory={}", params.memory));

        if let Some(ref name) = params.name {
            args.push(format!("name={}", name));
        }

        if let Some(ref disk) = params.disk {
            args.push(format!("disk={}", disk));
        } else if let Some(ref storage) = params.storage {
            let disk_size = match params.vm_type {
                VmType::Qemu => "8",
                VmType::Lxc => "4",
            };
            args.push(format!("disk={storage}:{disk_size}"));
        }

        if let Some(ref net) = params.net {
            args.push(format!("net0={}", net));
        }

        if params.onboot {
            args.push("onboot=1".to_string());
        }

        if let Some(ref description) = params.description {
            args.push(format!("description={}", description));
        }

        if let Some(ref pool) = params.pool {
            args.push(format!("pool={}", pool));
        }

        if let Some(cpuunits) = params.cpuunits {
            args.push(format!("cpuunits={}", cpuunits));
        }

        if let Some(cpulimit) = params.cpulimit {
            args.push(format!("cpulimit={}", cpulimit));
        }

        match params.vm_type {
            VmType::Qemu => {
                if let Some(ref ostype) = params.ostype {
                    args.push(format!("ostype={}", ostype));
                }
            }
            VmType::Lxc => {
                if let Some(ref template) = params.template {
                    args.push(format!("template={}", template));
                }
                if let Some(ref password) = params.password {
                    args.push(format!("password={}", password));
                }
                if params.unprivileged {
                    args.push("unprivileged=1".to_string());
                }
                if let Some(ref hostname) = params.hostname {
                    args.push(format!("hostname={}", hostname));
                }
                if let Some(swap) = params.swap {
                    args.push(format!("swap={}", swap));
                }
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_pvesh(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn start_vm(&self, node: &str, vmid: u32, vm_type: &VmType) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if self.is_running(node, vmid, vm_type)? {
            return Ok(false);
        }

        let path = match vm_type {
            VmType::Qemu => format!("/nodes/{node}/qemu/{vmid}/status/start"),
            VmType::Lxc => format!("/nodes/{node}/lxc/{vmid}/status/start"),
        };

        self.exec_pvesh(&["create", &path], true)?;
        Ok(true)
    }

    fn stop_vm(
        &self,
        node: &str,
        vmid: u32,
        vm_type: &VmType,
        force: bool,
        timeout: u32,
    ) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.is_running(node, vmid, vm_type)? {
            return Ok(false);
        }

        let path = match vm_type {
            VmType::Qemu => format!("/nodes/{node}/qemu/{vmid}/status/stop"),
            VmType::Lxc => format!("/nodes/{node}/lxc/{vmid}/status/stop"),
        };

        let mut args: Vec<String> = vec!["create".to_string(), path.clone()];
        if force {
            args.push("forceStop=1".to_string());
        }
        args.push(format!("timeout={}", timeout));

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_pvesh(&args_refs, true)?;
        Ok(true)
    }

    fn restart_vm(&self, node: &str, vmid: u32, vm_type: &VmType) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let path = match vm_type {
            VmType::Qemu => format!("/nodes/{node}/qemu/{vmid}/status/reboot"),
            VmType::Lxc => format!("/nodes/{node}/lxc/{vmid}/status/reboot"),
        };

        self.exec_pvesh(&["create", &path], true)?;
        Ok(true)
    }

    fn remove_vm(&self, node: &str, vmid: u32, vm_type: &VmType, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.vm_exists(node, vmid, vm_type)? {
            return Ok(false);
        }

        if self.is_running(node, vmid, vm_type)? && !force {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("VM/Container {vmid} is running. Use force=true to remove."),
            ));
        }

        if force && self.is_running(node, vmid, vm_type)? {
            self.stop_vm(node, vmid, vm_type, true, 10)?;
        }

        let path = match vm_type {
            VmType::Qemu => format!("/nodes/{node}/qemu/{vmid}"),
            VmType::Lxc => format!("/nodes/{node}/lxc/{vmid}"),
        };

        let args: Vec<&str> = vec!["delete", &path];
        self.exec_pvesh(&args, true)?;
        Ok(true)
    }

    fn clone_vm(&self, params: &Params, source_vmid: u32, new_vmid: u32) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let path = match params.vm_type {
            VmType::Qemu => format!("/nodes/{}/qemu/{}/clone", params.node, source_vmid),
            VmType::Lxc => format!("/nodes/{}/lxc/{}/clone", params.node, source_vmid),
        };

        let mut args: Vec<String> = vec!["create".to_string(), path.clone()];
        args.push(format!("newid={}", new_vmid));

        if let Some(ref name) = params.name {
            args.push(format!("name={}", name));
        }

        if let Some(ref description) = params.description {
            args.push(format!("description={}", description));
        }

        if let Some(ref pool) = params.pool {
            args.push(format!("pool={}", pool));
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_pvesh(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn get_vm_info(
        &self,
        node: &str,
        vmid: u32,
        vm_type: &VmType,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let path = match vm_type {
            VmType::Qemu => format!("/nodes/{node}/qemu/{vmid}/status/current"),
            VmType::Lxc => format!("/nodes/{node}/lxc/{vmid}/status/current"),
        };

        let output = self.exec_pvesh(&["get", &path], false)?;
        let mut result = serde_json::Map::new();

        if output.status.success() {
            result.insert("vmid".to_string(), serde_json::Value::Number(vmid.into()));
            result.insert(
                "node".to_string(),
                serde_json::Value::String(node.to_string()),
            );
            result.insert(
                "type".to_string(),
                serde_json::Value::String(vm_type.to_string()),
            );
            result.insert("exists".to_string(), serde_json::Value::Bool(true));

            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();
                    match key {
                        "status" => {
                            result.insert(
                                "status".to_string(),
                                serde_json::Value::String(value.to_string()),
                            );
                        }
                        "name" => {
                            result.insert(
                                "name".to_string(),
                                serde_json::Value::String(value.to_string()),
                            );
                        }
                        "vmid" => {
                            if let Ok(v) = value.parse::<u32>() {
                                result.insert(
                                    "vmid".to_string(),
                                    serde_json::Value::Number(v.into()),
                                );
                            }
                        }
                        "uptime" => {
                            if let Ok(v) = value.parse::<u64>() {
                                result.insert(
                                    "uptime".to_string(),
                                    serde_json::Value::Number(v.into()),
                                );
                            }
                        }
                        "cpu" => {
                            if let Ok(v) = value.parse::<f64>() {
                                result.insert("cpu".to_string(), serde_json::json!(v));
                            }
                        }
                        "mem" => {
                            if let Ok(v) = value.parse::<u64>() {
                                result
                                    .insert("mem".to_string(), serde_json::Value::Number(v.into()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

impl Module for Proxmox {
    fn get_name(&self) -> &str {
        "proxmox"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((proxmox(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn validate_node_name(node: &str) -> Result<()> {
    if node.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Node name cannot be empty",
        ));
    }

    if node.len() > 64 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Node name too long (max 64 characters)",
        ));
    }

    Ok(())
}

fn validate_vmid(vmid: u32) -> Result<()> {
    if !(100..=999999999).contains(&vmid) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "VMID must be between 100 and 999999999",
        ));
    }

    Ok(())
}

fn resolve_vmid(client: &ProxmoxClient, params: &Params) -> Result<u32> {
    if let Some(vmid) = params.vmid {
        validate_vmid(vmid)?;
        return Ok(vmid);
    }

    if let Some(ref name) = params.name
        && let Some(vm_info) = client.find_vm_by_name(name)?
    {
        return Ok(vm_info.vmid);
    }

    client.get_next_vmid()
}

fn proxmox(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_node_name(&params.node)?;

    let client = ProxmoxClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    let vmid = resolve_vmid(&client, &params)?;
    let vm_type = &params.vm_type;

    match params.state {
        State::Absent => {
            if client.remove_vm(&params.node, vmid, vm_type, params.force)? {
                output_messages.push(format!(
                    "{} {} removed from node {}",
                    vm_type, vmid, params.node
                ));
                changed = true;
            } else if !check_mode {
                output_messages.push(format!(
                    "{} {} not found on node {}",
                    vm_type, vmid, params.node
                ));
            }
        }
        State::Present => {
            let exists = client.vm_exists(&params.node, vmid, vm_type)?;
            if !exists {
                client.create_vm(&params, vmid)?;
                output_messages.push(format!(
                    "{} {} created on node {}",
                    vm_type, vmid, params.node
                ));
                changed = true;
            } else if !check_mode {
                output_messages.push(format!(
                    "{} {} already exists on node {}",
                    vm_type, vmid, params.node
                ));
            }
        }
        State::Started => {
            let exists = client.vm_exists(&params.node, vmid, vm_type)?;
            if !exists {
                client.create_vm(&params, vmid)?;
                output_messages.push(format!(
                    "{} {} created on node {}",
                    vm_type, vmid, params.node
                ));
                changed = true;
            }

            if client.start_vm(&params.node, vmid, vm_type)? {
                output_messages.push(format!(
                    "{} {} started on node {}",
                    vm_type, vmid, params.node
                ));
                changed = true;
            } else if !check_mode {
                output_messages.push(format!(
                    "{} {} already running on node {}",
                    vm_type, vmid, params.node
                ));
            }
        }
        State::Stopped => {
            let exists = client.vm_exists(&params.node, vmid, vm_type)?;
            if !exists {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "{} {} does not exist on node {}",
                        vm_type, vmid, params.node
                    ),
                ));
            }

            if client.stop_vm(&params.node, vmid, vm_type, params.force, params.timeout)? {
                output_messages.push(format!(
                    "{} {} stopped on node {}",
                    vm_type, vmid, params.node
                ));
                changed = true;
            } else if !check_mode {
                output_messages.push(format!(
                    "{} {} already stopped on node {}",
                    vm_type, vmid, params.node
                ));
            }
        }
        State::Restarted => {
            let exists = client.vm_exists(&params.node, vmid, vm_type)?;
            if !exists {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "{} {} does not exist on node {}",
                        vm_type, vmid, params.node
                    ),
                ));
            }

            client.restart_vm(&params.node, vmid, vm_type)?;
            output_messages.push(format!(
                "{} {} restarted on node {}",
                vm_type, vmid, params.node
            ));
            changed = true;
        }
        State::Cloned => {
            let source = params.source.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "source parameter is required for clone operation",
                )
            })?;

            let source_info = client.find_vm_by_name(source)?;
            let source_vmid = match source_info {
                Some(info) => info.vmid,
                None => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Source VM/Container '{}' not found", source),
                    ));
                }
            };

            client.clone_vm(&params, source_vmid, vmid)?;
            output_messages.push(format!(
                "{} {} cloned from {} ({}) on node {}",
                vm_type, vmid, source, source_vmid, params.node
            ));
            changed = true;
        }
    }

    let extra = client.get_vm_info(&params.node, vmid, vm_type)?;

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult::new(
        changed,
        Some(value::to_value(extra)?),
        final_output,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            node: pve1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("myvm".to_string()));
        assert_eq!(params.node, "pve1");
        assert_eq!(params.vm_type, VmType::Qemu);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.cores, 1);
        assert_eq!(params.memory, 512);
    }

    #[test]
    fn test_parse_params_full_vm() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            node: pve1
            type: qemu
            state: started
            cores: 4
            memory: 8192
            disk: "local-lvm:50"
            net: "virtio,bridge=vmbr0"
            ostype: l26
            onboot: true
            description: "My production VM"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("myvm".to_string()));
        assert_eq!(params.node, "pve1");
        assert_eq!(params.vm_type, VmType::Qemu);
        assert_eq!(params.state, State::Started);
        assert_eq!(params.cores, 4);
        assert_eq!(params.memory, 8192);
        assert_eq!(params.disk, Some("local-lvm:50".to_string()));
        assert_eq!(params.net, Some("virtio,bridge=vmbr0".to_string()));
        assert_eq!(params.ostype, Some("l26".to_string()));
        assert!(params.onboot);
    }

    #[test]
    fn test_parse_params_container() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myct
            node: pve1
            type: lxc
            state: started
            cores: 2
            memory: 1024
            disk: "local-lvm:8"
            template: "local:vztmpl/ubuntu-22.04.tar.gz"
            password: secret123
            unprivileged: true
            hostname: mycontainer
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vm_type, VmType::Lxc);
        assert_eq!(
            params.template,
            Some("local:vztmpl/ubuntu-22.04.tar.gz".to_string())
        );
        assert_eq!(params.password, Some("secret123".to_string()));
        assert!(params.unprivileged);
        assert_eq!(params.hostname, Some("mycontainer".to_string()));
    }

    #[test]
    fn test_parse_params_with_vmid() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vmid: 100
            node: pve1
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vmid, Some(100));
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_clone() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: cloned_vm
            node: pve1
            type: qemu
            state: cloned
            source: template_vm
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Cloned);
        assert_eq!(params.source, Some("template_vm".to_string()));
    }

    #[test]
    fn test_parse_params_stopped() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            node: pve1
            state: stopped
            force: true
            timeout: 120
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Stopped);
        assert!(params.force);
        assert_eq!(params.timeout, 120);
    }

    #[test]
    fn test_parse_params_restarted() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            node: pve1
            state: restarted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Restarted);
    }

    #[test]
    fn test_parse_params_pool() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            node: pve1
            pool: production
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.pool, Some("production".to_string()));
    }

    #[test]
    fn test_validate_node_name() {
        assert!(validate_node_name("pve1").is_ok());
        assert!(validate_node_name("node-01").is_ok());
        assert!(validate_node_name("proxmox-node").is_ok());

        assert!(validate_node_name("").is_err());
        assert!(validate_node_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn test_validate_vmid() {
        assert!(validate_vmid(100).is_ok());
        assert!(validate_vmid(1000).is_ok());
        assert!(validate_vmid(999999999).is_ok());

        assert!(validate_vmid(99).is_err());
        assert!(validate_vmid(0).is_err());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myvm
            node: pve1
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vm_type, VmType::Qemu);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.cores, 1);
        assert_eq!(params.memory, 512);
        assert!(!params.onboot);
        assert!(!params.unprivileged);
        assert!(!params.force);
        assert_eq!(params.timeout, 60);
    }

    #[test]
    fn test_vm_type_display() {
        assert_eq!(VmType::Qemu.to_string(), "qemu");
        assert_eq!(VmType::Lxc.to_string(), "lxc");
    }
}
