/// ANCHOR: module
/// # cgroups
///
/// Manage Linux control groups (cgroups) for resource management.
///
/// Supports both cgroup v1 and v2 filesystem-based management.
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
/// - name: Create a cgroup with CPU and memory limits
///   cgroups:
///     name: myapp
///     state: present
///     cpu_quota: 50000
///     memory_limit: 536870912
///
/// - name: Create a cgroup with all resource limits
///   cgroups:
///     name: constrained
///     state: present
///     cpu_quota: 25000
///     memory_limit: 268435456
///     io_weight: 100
///     pids_limit: 100
///     type: v2
///
/// - name: Attach processes to cgroup
///   cgroups:
///     name: myapp
///     state: attached
///     tasks:
///       - 1234
///       - 5678
///
/// - name: Detach all processes from cgroup
///   cgroups:
///     name: myapp
///     state: detached
///
/// - name: Remove a cgroup
///   cgroups:
///     name: myapp
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const CGROUP_V2_BASE: &str = "/sys/fs/cgroup";
const CGROUP_V1_BASE: &str = "/sys/fs/cgroup";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the cgroup to manage.
    pub name: String,
    /// Desired state of the cgroup.
    /// `present` creates the cgroup with specified limits.
    /// `absent` removes the cgroup.
    /// `attached` attaches processes to the cgroup.
    /// `detached` moves all processes out of the cgroup.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// CPU quota in microseconds per period (cgroup v2: cpu.max, v1: cpu.cfs_quota_us).
    /// A value of -1 removes the quota limit.
    pub cpu_quota: Option<i64>,
    /// Memory limit in bytes (cgroup v2: memory.max, v1: memory.limit_in_bytes).
    /// Use "max" for no limit.
    pub memory_limit: Option<String>,
    /// IO weight between 10 and 1000 (cgroup v2: io.bfq.weight, v1: blkio.weight).
    pub io_weight: Option<u32>,
    /// Maximum number of PIDs (cgroup v2: pids.max, v1: pids/pids.max).
    pub pids_limit: Option<u64>,
    /// List of process IDs to attach to the cgroup.
    /// Only used with state=attached.
    pub tasks: Option<Vec<u32>>,
    /// Cgroup version to use: v1 or v2.
    /// If not specified, auto-detects from the system.
    /// **[default: auto-detected]**
    #[serde(rename = "type")]
    pub cgroup_type: Option<CgroupVersion>,
    /// Base path for cgroup filesystem.
    /// **[default: `"/sys/fs/cgroup"`]**
    pub cgroup_path: Option<String>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
    Attached,
    Detached,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum CgroupVersion {
    #[default]
    V2,
    V1,
}

fn detect_cgroup_version(base_path: &str) -> Result<CgroupVersion> {
    let unified = Path::new(base_path).join("cgroup.controllers");
    if unified.exists() {
        return Ok(CgroupVersion::V2);
    }
    let cpu_controller = Path::new(base_path).join("cpu");
    let memory_controller = Path::new(base_path).join("memory");
    if cpu_controller.exists() || memory_controller.exists() {
        return Ok(CgroupVersion::V1);
    }
    Ok(CgroupVersion::V2)
}

fn get_cgroup_path_v2(base_path: &str, name: &str) -> PathBuf {
    PathBuf::from(base_path).join(name)
}

fn get_cgroup_path_v1(base_path: &str, name: &str, controller: &str) -> PathBuf {
    PathBuf::from(base_path).join(controller).join(name)
}

fn write_cgroup_file(path: &Path, value: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new().write(true).open(path).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to open {:?}: {e}", path),
        )
    })?;
    file.write_all(value.as_bytes()).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to write to {:?}: {e}", path),
        )
    })
}

fn read_cgroup_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to read {:?}: {e}", path),
        )
    })
}

#[allow(dead_code)]
fn cgroup_exists_v2(base_path: &str, name: &str) -> bool {
    get_cgroup_path_v2(base_path, name).exists()
}

fn cgroup_exists_v1(base_path: &str, name: &str) -> bool {
    get_cgroup_path_v1(base_path, name, "cpu").exists()
}

fn read_current_v2(cgroup_path: &Path) -> serde_norway::Mapping {
    let mut mapping = serde_norway::Mapping::new();

    if let Ok(val) = read_cgroup_file(&cgroup_path.join("cpu.max")) {
        let trimmed = val.trim();
        mapping.insert(
            serde_norway::Value::String("cpu_max".to_string()),
            serde_norway::Value::String(trimmed.to_string()),
        );
    }

    if let Ok(val) = read_cgroup_file(&cgroup_path.join("memory.max")) {
        let trimmed = val.trim();
        mapping.insert(
            serde_norway::Value::String("memory_max".to_string()),
            serde_norway::Value::String(trimmed.to_string()),
        );
    }

    if let Ok(val) = read_cgroup_file(&cgroup_path.join("memory.current")) {
        let trimmed = val.trim();
        mapping.insert(
            serde_norway::Value::String("memory_current".to_string()),
            serde_norway::Value::String(trimmed.to_string()),
        );
    }

    if let Ok(val) = read_cgroup_file(&cgroup_path.join("pids.max")) {
        let trimmed = val.trim();
        mapping.insert(
            serde_norway::Value::String("pids_max".to_string()),
            serde_norway::Value::String(trimmed.to_string()),
        );
    }

    if let Ok(val) = read_cgroup_file(&cgroup_path.join("pids.current")) {
        let trimmed = val.trim();
        mapping.insert(
            serde_norway::Value::String("pids_current".to_string()),
            serde_norway::Value::String(trimmed.to_string()),
        );
    }

    if let Ok(val) = read_cgroup_file(&cgroup_path.join("cgroup.procs")) {
        let procs: Vec<serde_norway::Value> = val
            .trim()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_norway::Value::String(l.to_string()))
            .collect();
        mapping.insert(
            serde_norway::Value::String("procs".to_string()),
            serde_norway::Value::Sequence(procs),
        );
    }

    mapping
}

fn read_current_v1(base_path: &str, name: &str) -> serde_norway::Mapping {
    let mut mapping = serde_norway::Mapping::new();

    let cpu_path = get_cgroup_path_v1(base_path, name, "cpu");
    if cpu_path.exists() {
        if let Ok(val) = read_cgroup_file(&cpu_path.join("cpu.cfs_quota_us")) {
            mapping.insert(
                serde_norway::Value::String("cpu_quota".to_string()),
                serde_norway::Value::String(val.trim().to_string()),
            );
        }
        if let Ok(val) = read_cgroup_file(&cpu_path.join("tasks")) {
            let procs: Vec<serde_norway::Value> = val
                .trim()
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| serde_norway::Value::String(l.to_string()))
                .collect();
            mapping.insert(
                serde_norway::Value::String("cpu_tasks".to_string()),
                serde_norway::Value::Sequence(procs),
            );
        }
    }

    let mem_path = get_cgroup_path_v1(base_path, name, "memory");
    if mem_path.exists() {
        if let Ok(val) = read_cgroup_file(&mem_path.join("memory.limit_in_bytes")) {
            mapping.insert(
                serde_norway::Value::String("memory_limit".to_string()),
                serde_norway::Value::String(val.trim().to_string()),
            );
        }
        if let Ok(val) = read_cgroup_file(&mem_path.join("memory.usage_in_bytes")) {
            mapping.insert(
                serde_norway::Value::String("memory_usage".to_string()),
                serde_norway::Value::String(val.trim().to_string()),
            );
        }
    }

    let blkio_path = get_cgroup_path_v1(base_path, name, "blkio");
    if blkio_path.exists()
        && let Ok(val) = read_cgroup_file(&blkio_path.join("blkio.weight"))
    {
        mapping.insert(
            serde_norway::Value::String("io_weight".to_string()),
            serde_norway::Value::String(val.trim().to_string()),
        );
    }

    let pids_path = get_cgroup_path_v1(base_path, name, "pids");
    if pids_path.exists() {
        if let Ok(val) = read_cgroup_file(&pids_path.join("pids.max")) {
            mapping.insert(
                serde_norway::Value::String("pids_max".to_string()),
                serde_norway::Value::String(val.trim().to_string()),
            );
        }
        if let Ok(val) = read_cgroup_file(&pids_path.join("pids.current")) {
            mapping.insert(
                serde_norway::Value::String("pids_current".to_string()),
                serde_norway::Value::String(val.trim().to_string()),
            );
        }
    }

    mapping
}

fn apply_v2_resource(
    cgroup_path: &Path,
    file_name: &str,
    new_val: &str,
    label: &str,
    check_mode: bool,
    changes: &mut Vec<String>,
) -> Result<bool> {
    let path = cgroup_path.join(file_name);
    if path.exists()
        && let Ok(current) = read_cgroup_file(&path)
    {
        let current_trimmed = current.trim().to_string();
        if current_trimmed != new_val {
            diff(&current_trimmed, new_val);
            if !check_mode {
                write_cgroup_file(&path, new_val)?;
            }
            changes.push(label.to_string());
            return Ok(true);
        }
    }
    Ok(false)
}

fn apply_v2(params: &Params, base_path: &str, check_mode: bool) -> Result<ModuleResult> {
    let cgroup_path = get_cgroup_path_v2(base_path, &params.name);
    let state = params.state.clone().unwrap_or_default();

    match state {
        State::Present => {
            let exists = cgroup_path.exists();
            let mut changed = !exists;
            let mut changes: Vec<String> = Vec::new();

            if !exists && !check_mode {
                fs::create_dir_all(&cgroup_path).map_err(|e| {
                    Error::new(
                        ErrorKind::IOError,
                        format!("Failed to create cgroup directory {:?}: {e}", cgroup_path),
                    )
                })?;
                changes.push(format!("created cgroup '{}'", params.name));
            } else if !exists {
                changes.push(format!("would create cgroup '{}'", params.name));
            }

            if cgroup_path.exists() || !check_mode {
                if let Some(quota) = params.cpu_quota {
                    let new_val = if quota < 0 {
                        "max".to_string()
                    } else {
                        format!("{quota} 100000")
                    };
                    if apply_v2_resource(
                        &cgroup_path,
                        "cpu.max",
                        &new_val,
                        &format!("cpu_quota -> {quota}"),
                        check_mode,
                        &mut changes,
                    )? {
                        changed = true;
                    }
                }

                if let Some(ref mem) = params.memory_limit
                    && apply_v2_resource(
                        &cgroup_path,
                        "memory.max",
                        mem,
                        &format!("memory_limit -> {mem}"),
                        check_mode,
                        &mut changes,
                    )?
                {
                    changed = true;
                }

                if let Some(weight) = params.io_weight {
                    if !(10..=1000).contains(&weight) {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("io_weight must be between 10 and 1000, got {weight}"),
                        ));
                    }
                    if apply_v2_resource(
                        &cgroup_path,
                        "io.bfq.weight",
                        &weight.to_string(),
                        &format!("io_weight -> {weight}"),
                        check_mode,
                        &mut changes,
                    )? {
                        changed = true;
                    }
                }

                if let Some(pids) = params.pids_limit
                    && apply_v2_resource(
                        &cgroup_path,
                        "pids.max",
                        &pids.to_string(),
                        &format!("pids_limit -> {pids}"),
                        check_mode,
                        &mut changes,
                    )?
                {
                    changed = true;
                }
            }

            let extra = if cgroup_path.exists() || !check_mode {
                Some(serde_norway::Value::Mapping(read_current_v2(&cgroup_path)))
            } else {
                None
            };

            let output = if changes.is_empty() {
                None
            } else {
                Some(changes.join(", "))
            };

            Ok(ModuleResult::new(changed, extra, output))
        }
        State::Absent => {
            if !cgroup_path.exists() {
                return Ok(ModuleResult::new(false, None, None));
            }

            if !check_mode {
                let procs_path = cgroup_path.join("cgroup.procs");
                if procs_path.exists()
                    && let Ok(procs) = read_cgroup_file(&procs_path)
                {
                    let parent_procs = Path::new(base_path).join("cgroup.procs");
                    for pid in procs.trim().lines().filter(|l| !l.is_empty()) {
                        if parent_procs.exists() {
                            let _ = write_cgroup_file(&parent_procs, pid);
                        }
                    }
                }

                fs::remove_dir_all(&cgroup_path).map_err(|e| {
                    Error::new(
                        ErrorKind::IOError,
                        format!("Failed to remove cgroup {:?}: {e}", cgroup_path),
                    )
                })?;
            }

            Ok(ModuleResult::new(
                true,
                None,
                Some(format!("removed cgroup '{}'", params.name)),
            ))
        }
        State::Attached => {
            let tasks = params.tasks.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "tasks parameter is required when state=attached",
                )
            })?;

            if !cgroup_path.exists() {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("cgroup '{}' does not exist", params.name),
                ));
            }

            let procs_path = cgroup_path.join("cgroup.procs");
            let existing = if procs_path.exists() {
                read_cgroup_file(&procs_path)
                    .unwrap_or_default()
                    .trim()
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let mut changed = false;
            let mut attached = Vec::new();
            for pid in tasks {
                let pid_str = pid.to_string();
                if !existing.contains(&pid_str) {
                    changed = true;
                    attached.push(*pid);
                    if !check_mode {
                        write_cgroup_file(&procs_path, &pid_str)?;
                    }
                }
            }

            let extra = if cgroup_path.exists() {
                Some(serde_norway::Value::Mapping(read_current_v2(&cgroup_path)))
            } else {
                None
            };

            let output = if attached.is_empty() {
                None
            } else {
                Some(format!(
                    "attached PIDs: {}",
                    attached
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            };

            Ok(ModuleResult::new(changed, extra, output))
        }
        State::Detached => {
            if !cgroup_path.exists() {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("cgroup '{}' does not exist", params.name),
                ));
            }

            let procs_path = cgroup_path.join("cgroup.procs");
            let procs_content = if procs_path.exists() {
                read_cgroup_file(&procs_path).unwrap_or_default()
            } else {
                String::new()
            };
            let existing: Vec<&str> = procs_content
                .trim()
                .lines()
                .filter(|l| !l.is_empty())
                .collect();

            if existing.is_empty() {
                return Ok(ModuleResult::new(false, None, None));
            }

            if !check_mode {
                let parent_procs = Path::new(base_path).join("cgroup.procs");
                for pid in &existing {
                    if parent_procs.exists() {
                        write_cgroup_file(&parent_procs, pid)?;
                    }
                }
            }

            Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "detached {} processes from cgroup '{}'",
                    existing.len(),
                    params.name
                )),
            ))
        }
    }
}

fn apply_v1_resource(
    cgroup_path: &Path,
    file_name: &str,
    new_val: &str,
    label: &str,
    check_mode: bool,
    changes: &mut Vec<String>,
) -> Result<bool> {
    let path = cgroup_path.join(file_name);
    if path.exists()
        && let Ok(current) = read_cgroup_file(&path)
    {
        let current_val = current.trim();
        if current_val != new_val {
            diff(current_val, new_val);
            if !check_mode {
                write_cgroup_file(&path, new_val)?;
            }
            changes.push(label.to_string());
            return Ok(true);
        }
    }
    Ok(false)
}

fn apply_v1(params: &Params, base_path: &str, check_mode: bool) -> Result<ModuleResult> {
    let state = params.state.clone().unwrap_or_default();
    let controllers = get_v1_controllers(params);

    match state {
        State::Present => {
            let mut changed = false;
            let mut changes: Vec<String> = Vec::new();

            for controller in &controllers {
                let cgroup_path = get_cgroup_path_v1(base_path, &params.name, controller);
                if !cgroup_path.exists() {
                    if !check_mode {
                        fs::create_dir_all(&cgroup_path).map_err(|e| {
                            Error::new(
                                ErrorKind::IOError,
                                format!("Failed to create cgroup dir {:?}: {e}", cgroup_path),
                            )
                        })?;
                    }
                    changed = true;
                    changes.push(format!("created {controller}/{name}", name = params.name));
                }
            }

            for controller in &controllers {
                let cgroup_path = get_cgroup_path_v1(base_path, &params.name, controller);

                if *controller == "cpu"
                    && let Some(quota) = params.cpu_quota
                    && apply_v1_resource(
                        &cgroup_path,
                        "cpu.cfs_quota_us",
                        &quota.to_string(),
                        &format!("cpu_quota -> {quota}"),
                        check_mode,
                        &mut changes,
                    )?
                {
                    changed = true;
                }

                if *controller == "memory"
                    && let Some(ref mem) = params.memory_limit
                    && apply_v1_resource(
                        &cgroup_path,
                        "memory.limit_in_bytes",
                        mem,
                        &format!("memory_limit -> {mem}"),
                        check_mode,
                        &mut changes,
                    )?
                {
                    changed = true;
                }

                if *controller == "blkio"
                    && let Some(weight) = params.io_weight
                {
                    if !(10..=1000).contains(&weight) {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("io_weight must be between 10 and 1000, got {weight}"),
                        ));
                    }
                    if apply_v1_resource(
                        &cgroup_path,
                        "blkio.weight",
                        &weight.to_string(),
                        &format!("io_weight -> {weight}"),
                        check_mode,
                        &mut changes,
                    )? {
                        changed = true;
                    }
                }

                if *controller == "pids"
                    && let Some(pids) = params.pids_limit
                    && apply_v1_resource(
                        &cgroup_path,
                        "pids.max",
                        &pids.to_string(),
                        &format!("pids_limit -> {pids}"),
                        check_mode,
                        &mut changes,
                    )?
                {
                    changed = true;
                }
            }

            let extra = Some(serde_norway::Value::Mapping(read_current_v1(
                base_path,
                &params.name,
            )));

            let output = if changes.is_empty() {
                None
            } else {
                Some(changes.join(", "))
            };

            Ok(ModuleResult::new(changed, extra, output))
        }
        State::Absent => {
            let exists = cgroup_exists_v1(base_path, &params.name);
            if !exists {
                return Ok(ModuleResult::new(false, None, None));
            }

            if !check_mode {
                for controller in &controllers {
                    let cgroup_path = get_cgroup_path_v1(base_path, &params.name, controller);
                    if cgroup_path.exists() {
                        let tasks_path = cgroup_path.join("tasks");
                        if tasks_path.exists()
                            && let Ok(tasks) = read_cgroup_file(&tasks_path)
                        {
                            let parent_tasks = Path::new(base_path).join(controller).join("tasks");
                            for pid in tasks.trim().lines().filter(|l| !l.is_empty()) {
                                if parent_tasks.exists() {
                                    let _ = write_cgroup_file(&parent_tasks, pid);
                                }
                            }
                        }

                        let _ = fs::remove_dir_all(&cgroup_path);
                    }
                }
            }

            Ok(ModuleResult::new(
                true,
                None,
                Some(format!("removed cgroup '{}'", params.name)),
            ))
        }
        State::Attached => {
            let tasks = params.tasks.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "tasks parameter is required when state=attached",
                )
            })?;

            let cpu_path = get_cgroup_path_v1(base_path, &params.name, "cpu");
            if !cpu_path.exists() {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("cgroup '{}' does not exist", params.name),
                ));
            }

            let mut changed = false;
            let mut attached = Vec::new();

            for pid in tasks {
                let pid_str = pid.to_string();
                let mut already_attached = false;

                for controller in &controllers {
                    let cgroup_path = get_cgroup_path_v1(base_path, &params.name, controller);
                    let tasks_path = cgroup_path.join("tasks");
                    if tasks_path.exists()
                        && let Ok(existing) = read_cgroup_file(&tasks_path)
                        && existing.trim().lines().any(|l| l == pid_str)
                    {
                        already_attached = true;
                    }
                }

                if !already_attached {
                    changed = true;
                    attached.push(*pid);
                    if !check_mode {
                        for controller in &controllers {
                            let cgroup_path =
                                get_cgroup_path_v1(base_path, &params.name, controller);
                            let tasks_path = cgroup_path.join("tasks");
                            if tasks_path.exists() {
                                write_cgroup_file(&tasks_path, &pid_str)?;
                            }
                        }
                    }
                }
            }

            let output = if attached.is_empty() {
                None
            } else {
                Some(format!(
                    "attached PIDs: {}",
                    attached
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            };

            Ok(ModuleResult::new(changed, None, output))
        }
        State::Detached => {
            let cpu_path = get_cgroup_path_v1(base_path, &params.name, "cpu");
            if !cpu_path.exists() {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("cgroup '{}' does not exist", params.name),
                ));
            }

            let tasks_path = cpu_path.join("tasks");
            let tasks_content = if tasks_path.exists() {
                read_cgroup_file(&tasks_path).unwrap_or_default()
            } else {
                String::new()
            };
            let existing: Vec<&str> = tasks_content
                .trim()
                .lines()
                .filter(|l| !l.is_empty())
                .collect();

            if existing.is_empty() {
                return Ok(ModuleResult::new(false, None, None));
            }

            if !check_mode {
                for controller in &controllers {
                    let cgroup_path = get_cgroup_path_v1(base_path, &params.name, controller);
                    let tasks_path = cgroup_path.join("tasks");
                    if tasks_path.exists()
                        && let Ok(pids) = read_cgroup_file(&tasks_path)
                    {
                        let parent_tasks = Path::new(base_path).join(controller).join("tasks");
                        for pid in pids.trim().lines().filter(|l| !l.is_empty()) {
                            if parent_tasks.exists() {
                                write_cgroup_file(&parent_tasks, pid)?;
                            }
                        }
                    }
                }
            }

            Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "detached {} processes from cgroup '{}'",
                    existing.len(),
                    params.name
                )),
            ))
        }
    }
}

fn get_v1_controllers(params: &Params) -> Vec<&'static str> {
    let mut controllers = Vec::new();
    if params.cpu_quota.is_some() {
        controllers.push("cpu");
    }
    if params.memory_limit.is_some() {
        controllers.push("memory");
    }
    if params.io_weight.is_some() {
        controllers.push("blkio");
    }
    if params.pids_limit.is_some() {
        controllers.push("pids");
    }
    if controllers.is_empty() {
        controllers = vec!["cpu", "memory"];
    }
    controllers
}

pub fn cgroups(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let base_path =
        params
            .cgroup_path
            .as_deref()
            .unwrap_or(if params.cgroup_type == Some(CgroupVersion::V1) {
                CGROUP_V1_BASE
            } else {
                CGROUP_V2_BASE
            });

    let version = match &params.cgroup_type {
        Some(v) => v.clone(),
        None => detect_cgroup_version(base_path)?,
    };

    match version {
        CgroupVersion::V2 => apply_v2(&params, base_path, check_mode),
        CgroupVersion::V1 => apply_v1(&params, base_path, check_mode),
    }
}

#[derive(Debug)]
pub struct Cgroups;

impl Module for Cgroups {
    fn get_name(&self) -> &str {
        "cgroups"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((cgroups(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn create_v2_cgroup_fs(dir: &std::path::Path) {
        fs::create_dir_all(dir.join("cgroup.controllers")).ok();
        fs::write(dir.join("cgroup.controllers"), "cpu memory io pids\n").ok();
    }

    fn create_v2_cgroup_with_files(dir: &std::path::Path) {
        fs::create_dir_all(dir).ok();
        fs::write(dir.join("cpu.max"), "max 100000\n").ok();
        fs::write(dir.join("memory.max"), "max\n").ok();
        fs::write(dir.join("memory.current"), "0\n").ok();
        fs::write(dir.join("pids.max"), "max\n").ok();
        fs::write(dir.join("pids.current"), "0\n").ok();
        fs::write(dir.join("cgroup.procs"), "\n").ok();
        fs::write(dir.join("cgroup.controllers"), "cpu memory io pids\n").ok();
    }

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: present
            cpu_quota: 50000
            memory_limit: "536870912"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.cpu_quota, Some(50000));
        assert_eq!(params.memory_limit, Some("536870912".to_string()));
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: testgroup
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "testgroup");
        assert_eq!(params.state, None);
        assert_eq!(params.cpu_quota, None);
    }

    #[test]
    fn test_parse_params_attached() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: attached
            tasks:
              - 1234
              - 5678
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Attached));
        assert_eq!(params.tasks, Some(vec![1234, 5678]));
    }

    #[test]
    fn test_parse_params_v1() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            type: v1
            cpu_quota: 25000
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.cgroup_type, Some(CgroupVersion::V1));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: oldgroup
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_detect_cgroup_version_v2() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("cgroup.controllers"), "cpu memory\n").ok();
        assert_eq!(
            detect_cgroup_version(base.to_str().unwrap()).unwrap(),
            CgroupVersion::V2
        );
    }

    #[test]
    fn test_detect_cgroup_version_v1() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        fs::create_dir_all(base.join("cpu")).ok();
        assert_eq!(
            detect_cgroup_version(base.to_str().unwrap()).unwrap(),
            CgroupVersion::V1
        );
    }

    #[test]
    fn test_detect_cgroup_version_default() {
        let dir = tempdir().unwrap();
        assert_eq!(
            detect_cgroup_version(dir.path().to_str().unwrap()).unwrap(),
            CgroupVersion::V2
        );
    }

    #[test]
    fn test_v2_present_creates_cgroup() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        create_v2_cgroup_fs(base);

        let cgroup_dir = base.join("myapp");
        create_v2_cgroup_with_files(&cgroup_dir);

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Present),
            cpu_quota: Some(50000),
            memory_limit: Some("536870912".to_string()),
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(result.changed);

        let cpu_max = fs::read_to_string(cgroup_dir.join("cpu.max")).unwrap();
        assert!(cpu_max.contains("50000 100000"));

        let mem_max = fs::read_to_string(cgroup_dir.join("memory.max")).unwrap();
        assert!(mem_max.contains("536870912"));
    }

    #[test]
    fn test_v2_present_no_change() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        create_v2_cgroup_fs(base);

        let cgroup_dir = base.join("myapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cpu.max"), "50000 100000\n").ok();
        fs::write(cgroup_dir.join("memory.max"), "536870912\n").ok();
        fs::write(cgroup_dir.join("memory.current"), "0\n").ok();
        fs::write(cgroup_dir.join("pids.max"), "max\n").ok();
        fs::write(cgroup_dir.join("pids.current"), "0\n").ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Present),
            cpu_quota: Some(50000),
            memory_limit: Some("536870912".to_string()),
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_v2_present_new_cgroup() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        create_v2_cgroup_fs(base);

        let cgroup_dir = base.join("newapp");
        create_v2_cgroup_with_files(&cgroup_dir);

        let params = Params {
            name: "newapp".to_string(),
            state: Some(State::Present),
            cpu_quota: Some(25000),
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, true).unwrap();
        assert!(result.changed);
    }

    #[test]
    fn test_v2_absent_removes_cgroup() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let cgroup_dir = base.join("myapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "\n").ok();
        fs::write(cgroup_dir.join("cpu.max"), "max 100000\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Absent),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(result.changed);
        assert!(!cgroup_dir.exists());
    }

    #[test]
    fn test_v2_absent_nonexistent() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let params = Params {
            name: "nonexistent".to_string(),
            state: Some(State::Absent),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_v2_attached() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let cgroup_dir = base.join("myapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "100\n").ok();
        fs::write(cgroup_dir.join("cpu.max"), "max 100000\n").ok();
        fs::write(cgroup_dir.join("memory.max"), "max\n").ok();
        fs::write(cgroup_dir.join("memory.current"), "0\n").ok();
        fs::write(cgroup_dir.join("pids.max"), "max\n").ok();
        fs::write(cgroup_dir.join("pids.current"), "1\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Attached),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: Some(vec![200, 300]),
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(result.changed);
    }

    #[test]
    fn test_v2_attached_already_attached() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let cgroup_dir = base.join("myapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "100\n200\n").ok();
        fs::write(cgroup_dir.join("cpu.max"), "max 100000\n").ok();
        fs::write(cgroup_dir.join("memory.max"), "max\n").ok();
        fs::write(cgroup_dir.join("memory.current"), "0\n").ok();
        fs::write(cgroup_dir.join("pids.max"), "max\n").ok();
        fs::write(cgroup_dir.join("pids.current"), "2\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Attached),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: Some(vec![100, 200]),
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_v2_attached_requires_tasks() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let cgroup_dir = base.join("myapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Attached),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("tasks parameter is required")
        );
    }

    #[test]
    fn test_v2_attached_nonexistent_cgroup() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let params = Params {
            name: "nonexistent".to_string(),
            state: Some(State::Attached),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: Some(vec![1234]),
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_v2_detached() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let cgroup_dir = base.join("myapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "100\n200\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Detached),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(result.changed);
    }

    #[test]
    fn test_v2_detached_empty() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let cgroup_dir = base.join("myapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Detached),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_v2_io_weight_validation() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        create_v2_cgroup_fs(base);

        let cgroup_dir = base.join("myapp");
        create_v2_cgroup_with_files(&cgroup_dir);
        fs::create_dir_all(cgroup_dir.join("io.bfq.weight")).ok();
        fs::write(cgroup_dir.join("io.bfq.weight"), "100\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Present),
            cpu_quota: None,
            memory_limit: None,
            io_weight: Some(5),
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("io_weight must be between 10 and 1000")
        );
    }

    #[test]
    fn test_v2_pids_limit() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        create_v2_cgroup_fs(base);

        let cgroup_dir = base.join("myapp");
        create_v2_cgroup_with_files(&cgroup_dir);

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Present),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: Some(100),
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(result.changed);

        let pids_max = fs::read_to_string(cgroup_dir.join("pids.max")).unwrap();
        assert!(pids_max.contains("100"));
    }

    #[test]
    fn test_v2_check_mode() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        create_v2_cgroup_fs(base);

        let cgroup_dir = base.join("myapp");
        create_v2_cgroup_with_files(&cgroup_dir);
        fs::write(cgroup_dir.join("cpu.max"), "max 100000\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Present),
            cpu_quota: Some(50000),
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V2),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, true).unwrap();
        assert!(result.changed);

        let cpu_max = fs::read_to_string(cgroup_dir.join("cpu.max")).unwrap();
        assert_eq!(cpu_max, "max 100000\n");
    }

    #[test]
    fn test_v1_present() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        fs::create_dir_all(base.join("cpu").join("myapp")).ok();
        fs::write(
            base.join("cpu").join("myapp").join("cpu.cfs_quota_us"),
            "-1\n",
        )
        .ok();
        fs::write(base.join("cpu").join("myapp").join("tasks"), "\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Present),
            cpu_quota: Some(50000),
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V1),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(result.changed);

        let quota =
            fs::read_to_string(base.join("cpu").join("myapp").join("cpu.cfs_quota_us")).unwrap();
        assert_eq!(quota.trim(), "50000");
    }

    #[test]
    fn test_v1_absent() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        let cpu_dir = base.join("cpu").join("myapp");
        fs::create_dir_all(&cpu_dir).ok();
        fs::write(cpu_dir.join("cpu.cfs_quota_us"), "-1\n").ok();
        fs::write(cpu_dir.join("tasks"), "\n").ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Absent),
            cpu_quota: None,
            memory_limit: None,
            io_weight: None,
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V1),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false).unwrap();
        assert!(result.changed);
        assert!(!cpu_dir.exists());
    }

    #[test]
    fn test_v1_io_weight_validation() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        fs::create_dir_all(base.join("blkio").join("myapp")).ok();
        fs::write(
            base.join("blkio").join("myapp").join("blkio.weight"),
            "500\n",
        )
        .ok();

        let params = Params {
            name: "myapp".to_string(),
            state: Some(State::Present),
            cpu_quota: None,
            memory_limit: None,
            io_weight: Some(5000),
            pids_limit: None,
            tasks: None,
            cgroup_type: Some(CgroupVersion::V1),
            cgroup_path: Some(base.to_str().unwrap().to_string()),
        };

        let result = cgroups(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("io_weight must be between 10 and 1000")
        );
    }

    #[test]
    fn test_read_current_v2() {
        let dir = tempdir().unwrap();
        let cgroup_dir = dir.path().join("testapp");
        fs::create_dir_all(&cgroup_dir).ok();
        fs::write(cgroup_dir.join("cpu.max"), "50000 100000\n").ok();
        fs::write(cgroup_dir.join("memory.max"), "536870912\n").ok();
        fs::write(cgroup_dir.join("memory.current"), "1024\n").ok();
        fs::write(cgroup_dir.join("pids.max"), "100\n").ok();
        fs::write(cgroup_dir.join("pids.current"), "5\n").ok();
        fs::write(cgroup_dir.join("cgroup.procs"), "100\n200\n").ok();

        let mapping = read_current_v2(&cgroup_dir);

        assert_eq!(
            mapping.get(serde_norway::Value::String("cpu_max".to_string())),
            Some(&serde_norway::Value::String("50000 100000".to_string()))
        );
        assert_eq!(
            mapping.get(serde_norway::Value::String("memory_max".to_string())),
            Some(&serde_norway::Value::String("536870912".to_string()))
        );
        assert_eq!(
            mapping.get(serde_norway::Value::String("memory_current".to_string())),
            Some(&serde_norway::Value::String("1024".to_string()))
        );
    }
}
