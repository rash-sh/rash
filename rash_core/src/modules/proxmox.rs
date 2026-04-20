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
/// ## Examples
///
/// ```yaml
/// - name: Create a container
///   proxmox:
///     node: pve1
///     vmid: 100
///     name: myapp
///     state: present
///     api_host: pve.local
///     api_user: root@pam
///     api_password: '{{ proxmox_password }}'
///
/// - name: Start a VM
///   proxmox:
///     node: pve1
///     vmid: 101
///     state: started
///     api_host: pve.local
///     api_user: root@pam
///     api_password: '{{ proxmox_password }}'
///
/// - name: Stop a container
///   proxmox:
///     node: pve1
///     vmid: 100
///     state: stopped
///     api_host: pve.local
///     api_user: root@pam
///     api_password: '{{ proxmox_password }}'
///
/// - name: Restart a VM
///   proxmox:
///     node: pve1
///     vmid: 101
///     state: restarted
///     api_host: pve.local
///     api_user: root@pam
///     api_password: '{{ proxmox_password }}'
///
/// - name: Remove a container
///   proxmox:
///     node: pve1
///     vmid: 100
///     state: absent
///     api_host: pve.local
///     api_user: root@pam
///     api_password: '{{ proxmox_password }}'
///
/// - name: Create VM from template with specific CPU and memory
///   proxmox:
///     node: pve1
///     vmid: 200
///     name: myvm
///     state: present
///     template: 9000
///     cores: 4
///     memory: 8192
///     api_host: pve.local
///     api_user: root@pam
///     api_password: '{{ proxmox_password }}'
///
/// - name: Use API token instead of password
///   proxmox:
///     node: pve1
///     vmid: 100
///     state: started
///     api_host: pve.local
///     api_user: root@pam
///     api_token_id: mytoken
///     api_token_secret: '{{ proxmox_token_secret }}'
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::env;

use log::trace;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    #[default]
    Started,
    Stopped,
    Restarted,
    Absent,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum VmType {
    #[default]
    Qemu,
    Lxc,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Proxmox node name.
    pub node: String,
    /// VM/Container ID.
    pub vmid: u32,
    /// VM/Container name.
    pub name: Option<String>,
    /// The desired state of the VM/Container.
    #[serde(default)]
    pub state: State,
    /// Type of VM (qemu for VMs, lxc for containers).
    #[serde(default)]
    pub vmtype: VmType,
    /// Proxmox API host URL.
    pub api_host: String,
    /// API username (e.g., root@pam).
    pub api_user: String,
    /// API password for authentication.
    pub api_password: Option<String>,
    /// API token ID for token-based authentication.
    pub api_token_id: Option<String>,
    /// API token secret for token-based authentication.
    pub api_token_secret: Option<String>,
    /// Template VMID to clone from.
    pub template: Option<u32>,
    /// Number of CPU cores.
    pub cores: Option<u32>,
    /// Memory in MB.
    pub memory: Option<u32>,
    /// Disk size (e.g., "8G").
    pub disk: Option<String>,
    /// Storage pool for disk.
    pub storage: Option<String>,
    /// Bridge network interface.
    pub bridge: Option<String>,
    /// IP address configuration (for containers).
    pub ip_address: Option<String>,
    /// Gateway IP address (for containers).
    pub gateway: Option<String>,
    /// OS template storage (for containers).
    pub ostemplate: Option<String>,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
    /// Wait for VM/container to be in desired state.
    #[serde(default = "default_wait")]
    pub wait: bool,
    /// Timeout for wait operations in seconds.
    #[serde(default = "default_timeout")]
    pub timeout: u32,
    /// Force stop/restart operations.
    #[serde(default)]
    pub force: bool,
    /// Description for the VM/container.
    pub description: Option<String>,
    /// Tags for the VM/container.
    pub tags: Option<String>,
    /// Pool to assign the VM/container to.
    pub pool: Option<String>,
}

fn default_validate_certs() -> bool {
    true
}

fn default_wait() -> bool {
    true
}

fn default_timeout() -> u32 {
    30
}

struct ProxmoxClient {
    api_host: String,
    api_user: String,
    api_password: Option<String>,
    api_token_id: Option<String>,
    api_token_secret: Option<String>,
    #[allow(dead_code)]
    validate_certs: bool,
    ticket: Option<String>,
    csrf_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthResponse {
    data: AuthData,
}

#[derive(Debug, Deserialize)]
struct AuthData {
    ticket: String,
    #[serde(rename = "CSRFPreventionToken")]
    csrf_prevention_token: String,
}

#[derive(Debug, Deserialize)]
struct VmStatusResponse {
    data: VmStatus,
}

#[derive(Debug, Deserialize)]
struct VmStatus {
    status: String,
    #[allow(dead_code)]
    vmid: u32,
    #[allow(dead_code)]
    name: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    running: bool,
}

#[derive(Debug, Deserialize)]
struct ClusterResourcesResponse {
    data: Vec<ClusterResource>,
}

#[derive(Debug, Deserialize)]
struct ClusterResource {
    #[serde(rename = "type")]
    resource_type: String,
    vmid: u32,
    node: Option<String>,
    name: Option<String>,
    status: Option<String>,
}

impl ProxmoxClient {
    fn new(params: &Params) -> Result<Self> {
        Ok(Self {
            api_host: params.api_host.clone(),
            api_user: params.api_user.clone(),
            api_password: params.api_password.clone(),
            api_token_id: params.api_token_id.clone(),
            api_token_secret: params.api_token_secret.clone(),
            validate_certs: params.validate_certs,
            ticket: None,
            csrf_token: None,
        })
    }

    fn get_client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(false)
            .build()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to create HTTP client: {e}"),
                )
            })
    }

    fn authenticate(&mut self) -> Result<()> {
        if self.api_token_id.is_some() && self.api_token_secret.is_some() {
            return Ok(());
        }

        let password = self.api_password.clone().or_else(|| {
            env::var("PROXMOX_PASSWORD").ok()
        }).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "API password not provided. Set 'api_password' parameter or PROXMOX_PASSWORD environment variable.",
            )
        })?;

        let client = Self::get_client()?;
        let url = format!(
            "https://{}/api2/json/access/ticket",
            self.api_host.trim_end_matches('/')
        );

        let form_body = format!(
            "username={}&password={}",
            urlencoding::encode(&self.api_user),
            urlencoding::encode(&password)
        );

        let response = client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Proxmox authentication failed: {e}"),
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Proxmox authentication failed with status {}: {}",
                    status, error_text
                ),
            ));
        }

        let auth_response: AuthResponse = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse authentication response: {e}"),
            )
        })?;

        self.ticket = Some(auth_response.data.ticket);
        self.csrf_token = Some(auth_response.data.csrf_prevention_token);

        Ok(())
    }

    fn build_request(&self, method: &str, path: &str) -> Result<reqwest::blocking::RequestBuilder> {
        let client = Self::get_client()?;
        let url = format!(
            "https://{}/api2/json/{path}",
            self.api_host.trim_end_matches('/')
        );

        let mut request = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "DELETE" => client.delete(&url),
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unsupported HTTP method: {method}"),
                ));
            }
        };

        if let (Some(token_id), Some(token_secret)) = (&self.api_token_id, &self.api_token_secret) {
            let token = format!("{}:{}!", self.api_user, token_id);
            request = request.header(
                "Authorization",
                format!("PVEAPIToken={token}={token_secret}"),
            );
        } else if let Some(ticket) = &self.ticket {
            request = request.header("Cookie", format!("PVEAuthCookie={ticket}"));
            if method != "GET"
                && let Some(csrf) = &self.csrf_token
            {
                request = request.header("CSRFPreventionToken", csrf);
            }
        }

        Ok(request)
    }

    fn get(&self, path: &str) -> Result<JsonValue> {
        let request = self.build_request("GET", path)?;
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Proxmox API request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Proxmox returned status {}: {}", status, error_text),
            ));
        }

        let json: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Proxmox response: {e}"),
            )
        })?;

        Ok(json)
    }

    fn post(&self, path: &str, data: Option<&HashMap<String, String>>) -> Result<JsonValue> {
        let mut request = self.build_request("POST", path)?;
        if let Some(d) = data {
            let json_data: HashMap<&str, &str> =
                d.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            request = request.json(&json_data);
        }
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Proxmox API POST request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Proxmox returned status {}: {}", status, error_text),
            ));
        }

        let json: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Proxmox response: {e}"),
            )
        })?;

        Ok(json)
    }

    #[allow(dead_code)]
    fn put(&self, path: &str, data: Option<&HashMap<String, String>>) -> Result<JsonValue> {
        let mut request = self.build_request("PUT", path)?;
        if let Some(d) = data {
            let json_data: HashMap<&str, &str> =
                d.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            request = request.json(&json_data);
        }
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Proxmox API PUT request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Proxmox returned status {}: {}", status, error_text),
            ));
        }

        let json: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Proxmox response: {e}"),
            )
        })?;

        Ok(json)
    }

    fn delete(&self, path: &str) -> Result<bool> {
        let request = self.build_request("DELETE", path)?;
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Proxmox API DELETE request failed: {e}"),
            )
        })?;

        Ok(response.status().is_success())
    }

    fn vm_exists(&mut self, node: &str, vmid: u32, vmtype: &VmType) -> Result<bool> {
        self.authenticate()?;
        let path = match vmtype {
            VmType::Qemu => format!("nodes/{node}/qemu/{vmid}/status/current"),
            VmType::Lxc => format!("nodes/{node}/lxc/{vmid}/status/current"),
        };
        let result = self.get(&path);
        match result {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == ErrorKind::SubprocessFail => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn get_vm_status(
        &mut self,
        node: &str,
        vmid: u32,
        vmtype: &VmType,
    ) -> Result<Option<VmStatus>> {
        self.authenticate()?;
        let path = match vmtype {
            VmType::Qemu => format!("nodes/{node}/qemu/{vmid}/status/current"),
            VmType::Lxc => format!("nodes/{node}/lxc/{vmid}/status/current"),
        };
        let result = self.get(&path)?;
        let status_response: VmStatusResponse = serde_json::from_value(result).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse VM status response: {e}"),
            )
        })?;
        Ok(Some(status_response.data))
    }

    #[allow(dead_code)]
    fn is_running(&mut self, node: &str, vmid: u32, vmtype: &VmType) -> Result<bool> {
        let status = self.get_vm_status(node, vmid, vmtype)?;
        Ok(status.is_some_and(|s| s.status == "running"))
    }

    fn find_vm_in_cluster(
        &mut self,
        vmid: u32,
        vmtype: &VmType,
    ) -> Result<Option<(String, VmStatus)>> {
        self.authenticate()?;
        let result = self.get("cluster/resources?type=vm")?;
        let resources: ClusterResourcesResponse = serde_json::from_value(result).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse cluster resources response: {e}"),
            )
        })?;

        let expected_type = match vmtype {
            VmType::Qemu => "qemu",
            VmType::Lxc => "lxc",
        };

        for resource in resources.data {
            if resource.vmid == vmid
                && resource.resource_type == expected_type
                && let Some(node) = resource.node
            {
                let status = VmStatus {
                    status: resource
                        .status
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    vmid: resource.vmid,
                    name: resource.name.clone(),
                    running: resource.status.as_deref() == Some("running"),
                };
                return Ok(Some((node, status)));
            }
        }
        Ok(None)
    }

    fn create_qemu_vm(&mut self, params: &Params) -> Result<bool> {
        self.authenticate()?;
        let mut data = HashMap::new();
        data.insert("vmid".to_string(), params.vmid.to_string());

        if let Some(ref name) = params.name {
            data.insert("name".to_string(), name.clone());
        }
        if let Some(cores) = params.cores {
            data.insert("cores".to_string(), cores.to_string());
        }
        if let Some(memory) = params.memory {
            data.insert("memory".to_string(), memory.to_string());
        }
        if let Some(ref disk) = params.disk {
            data.insert("disk0".to_string(), disk.clone());
        }
        if let Some(ref storage) = params.storage {
            data.insert("storage".to_string(), storage.clone());
        }
        if let Some(ref bridge) = params.bridge {
            data.insert("net0".to_string(), format!("model=virtio,bridge={bridge}"));
        }
        if let Some(ref description) = params.description {
            data.insert("description".to_string(), description.clone());
        }
        if let Some(ref tags) = params.tags {
            data.insert("tags".to_string(), tags.clone());
        }
        if let Some(ref pool) = params.pool {
            data.insert("pool".to_string(), pool.clone());
        }

        let path = format!("nodes/{}/qemu", params.node);
        self.post(&path, Some(&data))?;
        Ok(true)
    }

    fn clone_qemu_vm(&mut self, params: &Params, template_vmid: u32) -> Result<bool> {
        self.authenticate()?;
        let mut data = HashMap::new();
        data.insert("newid".to_string(), params.vmid.to_string());

        if let Some(ref name) = params.name {
            data.insert("name".to_string(), name.clone());
        }
        if let Some(cores) = params.cores {
            data.insert("cores".to_string(), cores.to_string());
        }
        if let Some(memory) = params.memory {
            data.insert("memory".to_string(), memory.to_string());
        }
        if let Some(ref description) = params.description {
            data.insert("description".to_string(), description.clone());
        }
        if let Some(ref pool) = params.pool {
            data.insert("pool".to_string(), pool.clone());
        }

        let path = format!("nodes/{}/qemu/{}/clone", params.node, template_vmid);
        self.post(&path, Some(&data))?;
        Ok(true)
    }

    fn create_lxc_container(&mut self, params: &Params) -> Result<bool> {
        self.authenticate()?;
        let mut data = HashMap::new();
        data.insert("vmid".to_string(), params.vmid.to_string());
        data.insert("ostype".to_string(), "debian".to_string());

        if let Some(ref name) = params.name {
            data.insert("hostname".to_string(), name.clone());
        }
        if let Some(ref ostemplate) = params.ostemplate {
            data.insert("ostemplate".to_string(), ostemplate.clone());
        }
        if let Some(memory) = params.memory {
            data.insert("memory".to_string(), memory.to_string());
        }
        if let Some(cores) = params.cores {
            data.insert("cores".to_string(), cores.to_string());
        }
        if let Some(ref storage) = params.storage {
            data.insert(
                "rootfs".to_string(),
                format!(
                    "{}:{}",
                    storage,
                    params.disk.clone().unwrap_or_else(|| "4G".to_string())
                ),
            );
        } else {
            data.insert(
                "rootfs".to_string(),
                format!(
                    "local:{}",
                    params.disk.clone().unwrap_or_else(|| "4G".to_string())
                ),
            );
        }
        if let Some(ref bridge) = params.bridge {
            data.insert(
                "net0".to_string(),
                format!("name=eth0,bridge={bridge},ip=dhcp"),
            );
        }
        if let Some(ref ip_address) = params.ip_address {
            if let Some(ref gateway) = params.gateway {
                data.insert(
                    "net0".to_string(),
                    format!(
                        "name=eth0,bridge={},ip={},gw={}",
                        params.bridge.clone().unwrap_or_else(|| "vmbr0".to_string()),
                        ip_address,
                        gateway
                    ),
                );
            } else {
                data.insert(
                    "net0".to_string(),
                    format!(
                        "name=eth0,bridge={},ip={}",
                        params.bridge.clone().unwrap_or_else(|| "vmbr0".to_string()),
                        ip_address
                    ),
                );
            }
        }
        if let Some(ref description) = params.description {
            data.insert("description".to_string(), description.clone());
        }
        if let Some(ref pool) = params.pool {
            data.insert("pool".to_string(), pool.clone());
        }

        let path = format!("nodes/{}/lxc", params.node);
        self.post(&path, Some(&data))?;
        Ok(true)
    }

    fn start_vm(&mut self, node: &str, vmid: u32, vmtype: &VmType) -> Result<bool> {
        self.authenticate()?;
        let path = match vmtype {
            VmType::Qemu => format!("nodes/{node}/qemu/{vmid}/status/start"),
            VmType::Lxc => format!("nodes/{node}/lxc/{vmid}/status/start"),
        };
        self.post(&path, None)?;
        Ok(true)
    }

    fn stop_vm(&mut self, node: &str, vmid: u32, vmtype: &VmType, force: bool) -> Result<bool> {
        self.authenticate()?;
        let path = match vmtype {
            VmType::Qemu => {
                if force {
                    format!("nodes/{node}/qemu/{vmid}/status/stop")
                } else {
                    format!("nodes/{node}/qemu/{vmid}/status/shutdown")
                }
            }
            VmType::Lxc => {
                if force {
                    format!("nodes/{node}/lxc/{vmid}/status/stop")
                } else {
                    format!("nodes/{node}/lxc/{vmid}/status/shutdown")
                }
            }
        };
        self.post(&path, None)?;
        Ok(true)
    }

    fn restart_vm(&mut self, node: &str, vmid: u32, vmtype: &VmType) -> Result<bool> {
        self.authenticate()?;
        let path = match vmtype {
            VmType::Qemu => format!("nodes/{node}/qemu/{vmid}/status/reboot"),
            VmType::Lxc => format!("nodes/{node}/lxc/{vmid}/status/reboot"),
        };
        self.post(&path, None)?;
        Ok(true)
    }

    fn delete_vm(&mut self, node: &str, vmid: u32, vmtype: &VmType) -> Result<bool> {
        self.authenticate()?;
        let path = match vmtype {
            VmType::Qemu => format!("nodes/{node}/qemu/{vmid}?purge=1"),
            VmType::Lxc => format!("nodes/{node}/lxc/{vmid}?purge=1"),
        };
        self.delete(&path)?;
        Ok(true)
    }

    fn wait_for_status(
        &mut self,
        node: &str,
        vmid: u32,
        vmtype: &VmType,
        desired_status: &str,
        timeout: u32,
    ) -> Result<bool> {
        let start = std::time::Instant::now();
        let timeout_secs = std::time::Duration::from_secs(u64::from(timeout));

        while start.elapsed() < timeout_secs {
            if let Some(status) = self.get_vm_status(node, vmid, vmtype)?
                && status.status == desired_status
            {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Timeout waiting for VM {} to reach status {}",
                vmid, desired_status
            ),
        ))
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some("VM/container would be created".to_string()),
        ));
    }

    let mut client = ProxmoxClient::new(params)?;

    if client.vm_exists(&params.node, params.vmid, &params.vmtype)? {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "vmid": params.vmid,
                "node": params.node,
                "exists": true
            }))?),
            Some("VM/container already exists".to_string()),
        ));
    }

    let changed = match params.vmtype {
        VmType::Qemu => {
            if let Some(template) = params.template {
                client.clone_qemu_vm(params, template)?
            } else {
                client.create_qemu_vm(params)?
            }
        }
        VmType::Lxc => client.create_lxc_container(params)?,
    };

    let extra = Some(value::to_value(json!({
        "vmid": params.vmid,
        "node": params.node,
        "vmtype": match params.vmtype {
            VmType::Qemu => "qemu",
            VmType::Lxc => "lxc",
        },
        "exists": true,
        "status": "stopped"
    }))?);

    Ok(ModuleResult::new(
        changed,
        extra,
        Some(format!("VM/container {} created", params.vmid)),
    ))
}

fn exec_started(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some("VM/container would be started".to_string()),
        ));
    }

    let mut client = ProxmoxClient::new(params)?;

    let vm_info = client.find_vm_in_cluster(params.vmid, &params.vmtype)?;
    let (node, status) = match vm_info {
        Some((n, s)) => (n, s),
        None => {
            if !client.vm_exists(&params.node, params.vmid, &params.vmtype)? {
                if let Some(template) = params.template {
                    match params.vmtype {
                        VmType::Qemu => client.clone_qemu_vm(params, template)?,
                        VmType::Lxc => client.create_lxc_container(params)?,
                    };
                } else {
                    match params.vmtype {
                        VmType::Qemu => client.create_qemu_vm(params)?,
                        VmType::Lxc => client.create_lxc_container(params)?,
                    };
                }
            }
            (
                params.node.clone(),
                VmStatus {
                    status: "stopped".to_string(),
                    vmid: params.vmid,
                    name: params.name.clone(),
                    running: false,
                },
            )
        }
    };

    let mut changed = false;
    if status.status != "running" {
        client.start_vm(&node, params.vmid, &params.vmtype)?;
        if params.wait {
            client.wait_for_status(
                &node,
                params.vmid,
                &params.vmtype,
                "running",
                params.timeout,
            )?;
        }
        changed = true;
    }

    let extra = Some(value::to_value(json!({
        "vmid": params.vmid,
        "node": node,
        "vmtype": match params.vmtype {
            VmType::Qemu => "qemu",
            VmType::Lxc => "lxc",
        },
        "status": "running",
        "running": true
    }))?);

    Ok(ModuleResult::new(
        changed,
        extra,
        Some(if changed {
            format!("VM/container {} started", params.vmid)
        } else {
            format!("VM/container {} already running", params.vmid)
        }),
    ))
}

fn exec_stopped(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some("VM/container would be stopped".to_string()),
        ));
    }

    let mut client = ProxmoxClient::new(params)?;

    let vm_info = client.find_vm_in_cluster(params.vmid, &params.vmtype)?;
    let (node, status) = match vm_info {
        Some((n, s)) => (n, s),
        None => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("VM/container {} not found", params.vmid),
            ));
        }
    };

    let mut changed = false;
    if status.status == "running" {
        client.stop_vm(&node, params.vmid, &params.vmtype, params.force)?;
        if params.wait {
            client.wait_for_status(
                &node,
                params.vmid,
                &params.vmtype,
                "stopped",
                params.timeout,
            )?;
        }
        changed = true;
    }

    let extra = Some(value::to_value(json!({
        "vmid": params.vmid,
        "node": node,
        "vmtype": match params.vmtype {
            VmType::Qemu => "qemu",
            VmType::Lxc => "lxc",
        },
        "status": "stopped",
        "running": false
    }))?);

    Ok(ModuleResult::new(
        changed,
        extra,
        Some(if changed {
            format!("VM/container {} stopped", params.vmid)
        } else {
            format!("VM/container {} already stopped", params.vmid)
        }),
    ))
}

fn exec_restarted(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some("VM/container would be restarted".to_string()),
        ));
    }

    let mut client = ProxmoxClient::new(params)?;

    let vm_info = client.find_vm_in_cluster(params.vmid, &params.vmtype)?;
    let node = match vm_info {
        Some((n, _)) => n,
        None => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("VM/container {} not found", params.vmid),
            ));
        }
    };

    client.restart_vm(&node, params.vmid, &params.vmtype)?;

    if params.wait {
        client.wait_for_status(
            &node,
            params.vmid,
            &params.vmtype,
            "running",
            params.timeout,
        )?;
    }

    let extra = Some(value::to_value(json!({
        "vmid": params.vmid,
        "node": node,
        "vmtype": match params.vmtype {
            VmType::Qemu => "qemu",
            VmType::Lxc => "lxc",
        },
        "status": "running",
        "running": true
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("VM/container {} restarted", params.vmid)),
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some("VM/container would be removed".to_string()),
        ));
    }

    let mut client = ProxmoxClient::new(params)?;

    let vm_info = client.find_vm_in_cluster(params.vmid, &params.vmtype)?;
    let (node, status) = match vm_info {
        Some((n, s)) => (n, s),
        None => {
            return Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "vmid": params.vmid,
                    "exists": false
                }))?),
                Some(format!("VM/container {} not found", params.vmid)),
            ));
        }
    };

    if status.status == "running" {
        client.stop_vm(&node, params.vmid, &params.vmtype, true)?;
        if params.wait {
            client.wait_for_status(
                &node,
                params.vmid,
                &params.vmtype,
                "stopped",
                params.timeout,
            )?;
        }
    }

    client.delete_vm(&node, params.vmid, &params.vmtype)?;

    let extra = Some(value::to_value(json!({
        "vmid": params.vmid,
        "node": node,
        "exists": false
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("VM/container {} removed", params.vmid)),
    ))
}

pub fn proxmox(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Started => exec_started(&params, check_mode),
        State::Stopped => exec_stopped(&params, check_mode),
        State::Restarted => exec_restarted(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct Proxmox;

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

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            name: myapp
            state: present
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.node, "pve1");
        assert_eq!(params.vmid, 100);
        assert_eq!(params.name, Some("myapp".to_string()));
        assert_eq!(params.state, State::Present);
        assert_eq!(params.api_host, "pve.local");
        assert_eq!(params.api_user, "root@pam");
        assert_eq!(params.api_password, Some("secret".to_string()));
        assert_eq!(params.vmtype, VmType::Qemu);
    }

    #[test]
    fn test_parse_params_started() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 101
            state: started
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Started);
        assert_eq!(params.name, None);
    }

    #[test]
    fn test_parse_params_stopped() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            state: stopped
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Stopped);
    }

    #[test]
    fn test_parse_params_restarted() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 101
            state: restarted
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Restarted);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            state: absent
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_lxc() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            name: mycontainer
            vmtype: lxc
            state: present
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            ostemplate: local:vztmpl/debian-12.tar.zst
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vmtype, VmType::Lxc);
        assert_eq!(
            params.ostemplate,
            Some("local:vztmpl/debian-12.tar.zst".to_string())
        );
    }

    #[test]
    fn test_parse_params_with_resources() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 200
            name: myvm
            state: present
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            cores: 4
            memory: 8192
            disk: 20G
            storage: local-lvm
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.cores, Some(4));
        assert_eq!(params.memory, Some(8192));
        assert_eq!(params.disk, Some("20G".to_string()));
        assert_eq!(params.storage, Some("local-lvm".to_string()));
    }

    #[test]
    fn test_parse_params_with_template() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 200
            name: clonedvm
            state: present
            template: 9000
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.template, Some(9000));
    }

    #[test]
    fn test_parse_params_with_network() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            vmtype: lxc
            state: present
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            bridge: vmbr0
            ip_address: 192.168.1.100/24
            gateway: 192.168.1.1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.bridge, Some("vmbr0".to_string()));
        assert_eq!(params.ip_address, Some("192.168.1.100/24".to_string()));
        assert_eq!(params.gateway, Some("192.168.1.1".to_string()));
    }

    #[test]
    fn test_parse_params_with_api_token() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            state: started
            api_host: pve.local
            api_user: root@pam
            api_token_id: mytoken
            api_token_secret: tokensecret123
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.api_token_id, Some("mytoken".to_string()));
        assert_eq!(params.api_token_secret, Some("tokensecret123".to_string()));
        assert_eq!(params.api_password, None);
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            state: started
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            validate_certs: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_parse_params_wait_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            state: started
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            wait: false
            timeout: 60
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.wait);
        assert_eq!(params.timeout, 60);
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            state: stopped
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_pool_and_tags() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            name: myvm
            state: present
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            pool: production
            tags: web;production
            description: My production web server
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.pool, Some("production".to_string()));
        assert_eq!(params.tags, Some("web;production".to_string()));
        assert_eq!(
            params.description,
            Some("My production web server".to_string())
        );
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.validate_certs);
        assert!(params.wait);
        assert_eq!(params.timeout, 30);
        assert_eq!(params.state, State::Started);
        assert_eq!(params.vmtype, VmType::Qemu);
        assert!(!params.force);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            node: pve1
            vmid: 100
            state: started
            api_host: pve.local
            api_user: root@pam
            api_password: secret
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
