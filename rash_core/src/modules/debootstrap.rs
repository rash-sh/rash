/// ANCHOR: module
/// # debootstrap
///
/// Install a minimal Debian/Ubuntu base system into a directory using debootstrap.
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
/// - name: Install Ubuntu Noble base system
///   debootstrap:
///     target: /mnt
///     suite: noble
///     mirror: http://archive.ubuntu.com/ubuntu
///     arch: amd64
///     variant: minbase
///     components:
///       - main
///       - universe
///     include:
///       - linux-image-generic
///       - locales
///       - sudo
///       - openssh-server
///
/// - name: Install from Hetzner image tarball
///   debootstrap:
///     target: /mnt
///     suite: noble
///     unpack_tarball: /root/.oldroot/nfs/images/Ubuntu-2404-noble-amd64-base.tar.gz
///
/// - name: Install Debian Bookworm
///   debootstrap:
///     target: /mnt
///     suite: bookworm
///     mirror: http://deb.debian.org/debian
///     variant: minbase
///
/// - name: Run second stage for foreign architecture
///   debootstrap:
///     target: /mnt
///     suite: noble
///     second_stage: true
///     second_stage_target: /mnt
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("debootstrap".to_owned())
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Variant {
    #[default]
    Minbase,
    Buildd,
    Fakechroot,
    Scratch,
}

fn default_variant() -> Option<Variant> {
    Some(Variant::default())
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Target directory for the base system installation.
    target: String,
    /// Distribution codename (e.g., noble, jammy, bookworm, bullseye).
    suite: String,
    /// Path of the debootstrap binary to use.
    /// **[default: `"debootstrap"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Archive mirror URL (e.g., http://archive.ubuntu.com/ubuntu).
    mirror: Option<String>,
    /// Architecture for the installation (e.g., amd64, arm64).
    arch: Option<String>,
    /// Bootstrap variant to use.
    /// **[default: `"minbase"`]**
    #[serde(default = "default_variant")]
    variant: Option<Variant>,
    /// Components to include in the installation.
    /// **[default: `["main"]`]**
    #[serde(default)]
    components: Vec<String>,
    /// Comma-separated list of packages to include.
    include: Option<String>,
    /// Comma-separated list of packages to exclude.
    exclude: Option<String>,
    /// Path to keyring file for archive signing keys.
    keyring: Option<String>,
    /// Skip GPG signature verification.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_check_gpg: Option<bool>,
    /// Don't resolve dependencies.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_resolve_deps: Option<bool>,
    /// Extract from tarball instead of downloading.
    unpack_tarball: Option<String>,
    /// Run second stage after first stage (for foreign architectures).
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    second_stage: Option<bool>,
    /// Target directory for second stage (for foreign architectures).
    second_stage_target: Option<String>,
    /// Keep /debootstrap directory after installation.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    keep_debootstrap_dir: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            target: String::new(),
            suite: String::new(),
            executable: Some("debootstrap".to_owned()),
            mirror: None,
            arch: None,
            variant: Some(Variant::Minbase),
            components: Vec::new(),
            include: None,
            exclude: None,
            keyring: None,
            no_check_gpg: Some(false),
            no_resolve_deps: Some(false),
            unpack_tarball: None,
            second_stage: Some(false),
            second_stage_target: None,
            keep_debootstrap_dir: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Debootstrap;

impl Module for Debootstrap {
    fn get_name(&self) -> &str {
        "debootstrap"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            debootstrap(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct DebootstrapClient {
    executable: String,
    check_mode: bool,
}

impl DebootstrapClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(DebootstrapClient {
            executable: params.executable.clone().unwrap(),
            check_mode,
        })
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable
                ),
            )
        })?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(output)
    }

    fn is_target_installed(&self, target: &str) -> Result<bool> {
        let debootstrap_dir = Path::new(target).join("debootstrap");
        Ok(debootstrap_dir.exists())
    }

    fn count_installed_packages(&self, target: &str) -> Result<usize> {
        let dpkg_status = Path::new(target).join("var/lib/dpkg/status");
        if !dpkg_status.exists() {
            return Ok(0);
        }

        let contents = std::fs::read_to_string(&dpkg_status).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to read dpkg status: {e}"),
            )
        })?;

        Ok(contents.matches("Package:").count())
    }

    pub fn run(&self, params: &Params) -> Result<ModuleResult> {
        let target = &params.target;
        let suite = &params.suite;

        if self.is_target_installed(target)? && !params.second_stage.unwrap_or(false) {
            let packages = self.count_installed_packages(target)?;
            return Ok(ModuleResult::new(
                false,
                Some(serde_norway::Value::Mapping({
                    let mut map = serde_norway::Mapping::new();
                    map.insert(
                        serde_norway::Value::String("target".to_owned()),
                        serde_norway::Value::String(target.clone()),
                    );
                    map.insert(
                        serde_norway::Value::String("suite".to_owned()),
                        serde_norway::Value::String(suite.clone()),
                    );
                    if let Some(arch) = &params.arch {
                        map.insert(
                            serde_norway::Value::String("arch".to_owned()),
                            serde_norway::Value::String(arch.clone()),
                        );
                    }
                    map.insert(
                        serde_norway::Value::String("packages_installed".to_owned()),
                        serde_norway::Value::Number(packages.into()),
                    );
                    map
                })),
                None,
            ));
        }

        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                Some(serde_norway::Value::Mapping({
                    let mut map = serde_norway::Mapping::new();
                    map.insert(
                        serde_norway::Value::String("target".to_owned()),
                        serde_norway::Value::String(target.clone()),
                    );
                    map.insert(
                        serde_norway::Value::String("suite".to_owned()),
                        serde_norway::Value::String(suite.clone()),
                    );
                    if let Some(arch) = &params.arch {
                        map.insert(
                            serde_norway::Value::String("arch".to_owned()),
                            serde_norway::Value::String(arch.clone()),
                        );
                    }
                    map.insert(
                        serde_norway::Value::String("packages_installed".to_owned()),
                        serde_norway::Value::Number(0.into()),
                    );
                    map
                })),
                None,
            ));
        }

        let mut cmd = Command::new(&self.executable);

        if let Some(arch) = &params.arch {
            cmd.arg("--arch").arg(arch);
        }

        if let Some(variant) = &params.variant {
            let variant_str = match variant {
                Variant::Minbase => "minbase",
                Variant::Buildd => "buildd",
                Variant::Fakechroot => "fakechroot",
                Variant::Scratch => "scratch",
            };
            cmd.arg("--variant").arg(variant_str);
        }

        if !params.components.is_empty() {
            cmd.arg("--components").arg(params.components.join(","));
        }

        if let Some(include) = &params.include {
            cmd.arg("--include").arg(include);
        }

        if let Some(exclude) = &params.exclude {
            cmd.arg("--exclude").arg(exclude);
        }

        if let Some(keyring) = &params.keyring {
            cmd.arg("--keyring").arg(keyring);
        }

        if params.no_check_gpg.unwrap_or(false) {
            cmd.arg("--no-check-gpg");
        }

        if params.no_resolve_deps.unwrap_or(false) {
            cmd.arg("--no-resolve-deps");
        }

        if let Some(tarball) = &params.unpack_tarball {
            cmd.arg("--unpack-tarball").arg(tarball);
        }

        if params.second_stage.unwrap_or(false) {
            cmd.arg("--second-stage");
            if let Some(second_stage_target) = &params.second_stage_target {
                cmd.arg("--second-stage-target").arg(second_stage_target);
            }
        }

        if params.keep_debootstrap_dir.unwrap_or(false) {
            cmd.arg("--keep-debootstrap-dir");
        }

        cmd.arg(suite).arg(target);

        if let Some(mirror) = &params.mirror {
            cmd.arg(mirror);
        }

        logger::add(std::slice::from_ref(target));
        self.exec_cmd(&mut cmd)?;

        let packages = self.count_installed_packages(target)?;

        Ok(ModuleResult::new(
            true,
            Some(serde_norway::Value::Mapping({
                let mut map = serde_norway::Mapping::new();
                map.insert(
                    serde_norway::Value::String("target".to_owned()),
                    serde_norway::Value::String(target.clone()),
                );
                map.insert(
                    serde_norway::Value::String("suite".to_owned()),
                    serde_norway::Value::String(suite.clone()),
                );
                if let Some(arch) = &params.arch {
                    map.insert(
                        serde_norway::Value::String("arch".to_owned()),
                        serde_norway::Value::String(arch.clone()),
                    );
                }
                map.insert(
                    serde_norway::Value::String("packages_installed".to_owned()),
                    serde_norway::Value::Number(packages.into()),
                );
                map
            })),
            None,
        ))
    }
}

fn debootstrap(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let target_path = Path::new(&params.target);
    if !target_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Target directory '{}' does not exist", params.target),
        ));
    }

    let client = DebootstrapClient::new(&params, check_mode)?;
    client.run(&params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            target: /mnt
            suite: noble
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.target, "/mnt");
        assert_eq!(params.suite, "noble");
        assert_eq!(params.executable, Some("debootstrap".to_owned()));
        assert_eq!(params.variant, Some(Variant::Minbase));
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            target: /mnt
            suite: noble
            executable: /usr/sbin/debootstrap
            mirror: http://archive.ubuntu.com/ubuntu
            arch: amd64
            variant: minbase
            components:
              - main
              - universe
            include: linux-image-generic,locales
            exclude: nano
            keyring: /usr/share/keyrings/ubuntu-archive-keyring.gpg
            no_check_gpg: true
            no_resolve_deps: false
            unpack_tarball: /tmp/base.tar.gz
            second_stage: false
            second_stage_target: /mnt
            keep_debootstrap_dir: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.target, "/mnt");
        assert_eq!(params.suite, "noble");
        assert_eq!(params.executable, Some("/usr/sbin/debootstrap".to_owned()));
        assert_eq!(
            params.mirror,
            Some("http://archive.ubuntu.com/ubuntu".to_owned())
        );
        assert_eq!(params.arch, Some("amd64".to_owned()));
        assert_eq!(params.variant, Some(Variant::Minbase));
        assert_eq!(params.components, vec!["main", "universe"]);
        assert_eq!(
            params.include,
            Some("linux-image-generic,locales".to_owned())
        );
        assert_eq!(params.exclude, Some("nano".to_owned()));
        assert_eq!(
            params.keyring,
            Some("/usr/share/keyrings/ubuntu-archive-keyring.gpg".to_owned())
        );
        assert_eq!(params.no_check_gpg, Some(true));
        assert_eq!(params.no_resolve_deps, Some(false));
        assert_eq!(params.unpack_tarball, Some("/tmp/base.tar.gz".to_owned()));
        assert_eq!(params.second_stage, Some(false));
        assert_eq!(params.second_stage_target, Some("/mnt".to_owned()));
        assert_eq!(params.keep_debootstrap_dir, Some(true));
    }

    #[test]
    fn test_parse_params_variants() {
        let variants = ["minbase", "buildd", "fakechroot", "scratch"];
        for v in variants {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                target: /mnt
                suite: noble
                variant: {}
                "#,
                v
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            let expected = match v {
                "minbase" => Variant::Minbase,
                "buildd" => Variant::Buildd,
                "fakechroot" => Variant::Fakechroot,
                "scratch" => Variant::Scratch,
                _ => unreachable!(),
            };
            assert_eq!(params.variant, Some(expected));
        }
    }

    #[test]
    fn test_parse_params_missing_required() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            target: /mnt
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            target: /mnt
            suite: noble
            unknown_field: value
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_debootstrap_client_is_target_installed_false() {
        let params = Params {
            target: "/nonexistent/path".to_owned(),
            ..Default::default()
        };
        let client = DebootstrapClient::new(&params, false).unwrap();
        assert!(!client.is_target_installed("/nonexistent/path").unwrap());
    }

    #[test]
    fn test_target_not_exists_error() {
        let params = Params {
            target: "/nonexistent/path".to_owned(),
            suite: "noble".to_owned(),
            ..Default::default()
        };
        let result = debootstrap(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }
}
