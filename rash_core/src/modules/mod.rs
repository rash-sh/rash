mod alternatives;
mod apk;
mod apt;
mod apt_repository;
mod archive;
mod assemble;
mod assert;
mod async_status;
mod at;
mod authorized_key;
mod block;
mod command;
mod copy;
mod cron;
mod dconf;
mod debconf;
mod debug;
mod dnf;
mod docker_container;
mod fail;
mod file;
mod filesystem;
pub mod find;
mod firewalld;
mod gem;
mod get_url;
mod git;
mod gpg_key;
mod group;
mod hostname;
mod include;
mod ini_file;
mod interfaces_file;
mod iptables;
mod kernel_blacklist;
mod lineinfile;
mod locale;
mod lvg;
mod lvol;
mod meta;
mod modprobe;
mod mount;
mod nmcli;
mod openssl_privatekey;
mod pacman;
mod pam_limits;
mod parted;
mod reboot;
mod script;
mod seboolean;
mod service;
mod set_vars;
mod setup;
mod slurp;
mod stat;
mod synchronize;
mod sysctl;
mod systemd;
mod template;
mod timezone;
mod trace;
mod unarchive;
mod uri;
mod user;
mod wait_for;
mod yum_repository;
mod zypper;

use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::alternatives::Alternatives;
use crate::modules::apk::Apk;
use crate::modules::apt::Apt;
use crate::modules::apt_repository::AptRepository;
use crate::modules::archive::Archive;
use crate::modules::assemble::Assemble;
use crate::modules::assert::Assert;
use crate::modules::async_status::{AsyncPoll, AsyncStatus};
use crate::modules::at::At;
use crate::modules::authorized_key::AuthorizedKey;
use crate::modules::block::Block;
use crate::modules::command::Command;
use crate::modules::copy::Copy;
use crate::modules::cron::Cron;
use crate::modules::dconf::Dconf;
use crate::modules::debconf::Debconf;
use crate::modules::debug::Debug;
use crate::modules::dnf::Dnf;
use crate::modules::docker_container::DockerContainer;
use crate::modules::fail::Fail;
use crate::modules::file::File;
use crate::modules::filesystem::Filesystem;
use crate::modules::find::Find;
use crate::modules::firewalld::Firewalld;
use crate::modules::gem::Gem;
use crate::modules::get_url::GetUrl;
use crate::modules::git::Git;
use crate::modules::gpg_key::GpgKey;
use crate::modules::group::Group;
use crate::modules::hostname::Hostname;
use crate::modules::include::Include;
use crate::modules::ini_file::IniFile;
use crate::modules::interfaces_file::InterfacesFile;
use crate::modules::iptables::Iptables;
use crate::modules::kernel_blacklist::KernelBlacklist;
use crate::modules::lineinfile::Lineinfile;
use crate::modules::locale::Locale;
use crate::modules::lvg::Lvg;
use crate::modules::lvol::Lvol;
use crate::modules::meta::Meta;
use crate::modules::modprobe::Modprobe;
use crate::modules::mount::Mount;
use crate::modules::nmcli::Nmcli;
use crate::modules::openssl_privatekey::OpensslPrivatekey;
use crate::modules::pacman::Pacman;
use crate::modules::pam_limits::PamLimits;
use crate::modules::parted::Parted;
use crate::modules::reboot::Reboot;
use crate::modules::script::Script;
use crate::modules::seboolean::Seboolean;
use crate::modules::service::Service;
use crate::modules::set_vars::SetVars;
use crate::modules::setup::Setup;
use crate::modules::slurp::Slurp;
use crate::modules::stat::Stat;
use crate::modules::synchronize::Synchronize;
use crate::modules::sysctl::Sysctl;
use crate::modules::systemd::Systemd;
use crate::modules::template::Template;
use crate::modules::timezone::Timezone;
use crate::modules::trace::Trace;
use crate::modules::unarchive::Unarchive;
use crate::modules::uri::Uri;
use crate::modules::user::User;
use crate::modules::wait_for::WaitFor;
use crate::modules::yum_repository::YumRepository;
use crate::modules::zypper::Zypper;

use std::collections::HashMap;
use std::sync::LazyLock;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::Schema;
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModuleResult {
    changed: bool,
    output: Option<String>,
    extra: Option<YamlValue>,
}

impl ModuleResult {
    pub fn new(changed: bool, extra: Option<YamlValue>, output: Option<String>) -> Self {
        Self {
            changed,
            extra,
            output,
        }
    }

    pub fn get_changed(&self) -> bool {
        self.changed
    }

    pub fn get_extra(&self) -> Option<YamlValue> {
        self.extra.clone()
    }

    pub fn get_output(&self) -> Option<String> {
        self.output.clone()
    }
}

pub trait Module: Send + Sync + std::fmt::Debug {
    fn get_name(&self) -> &str;

    fn exec(
        &self,
        global_params: &GlobalParams,
        params: YamlValue,
        vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)>;

    fn force_string_on_params(&self) -> bool {
        true
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema>;
}

pub static MODULES: LazyLock<HashMap<&'static str, Box<dyn Module>>> = LazyLock::new(|| {
    vec![
        (
            Alternatives.get_name(),
            Box::new(Alternatives) as Box<dyn Module>,
        ),
        (Apk.get_name(), Box::new(Apk) as Box<dyn Module>),
        (Apt.get_name(), Box::new(Apt) as Box<dyn Module>),
        (
            AptRepository.get_name(),
            Box::new(AptRepository) as Box<dyn Module>,
        ),
        (Archive.get_name(), Box::new(Archive) as Box<dyn Module>),
        (Assemble.get_name(), Box::new(Assemble) as Box<dyn Module>),
        (Assert.get_name(), Box::new(Assert) as Box<dyn Module>),
        (AsyncPoll.get_name(), Box::new(AsyncPoll) as Box<dyn Module>),
        (
            AsyncStatus.get_name(),
            Box::new(AsyncStatus) as Box<dyn Module>,
        ),
        (At.get_name(), Box::new(At) as Box<dyn Module>),
        (
            AuthorizedKey.get_name(),
            Box::new(AuthorizedKey) as Box<dyn Module>,
        ),
        (Block.get_name(), Box::new(Block) as Box<dyn Module>),
        (Command.get_name(), Box::new(Command) as Box<dyn Module>),
        (Copy.get_name(), Box::new(Copy) as Box<dyn Module>),
        (Cron.get_name(), Box::new(Cron) as Box<dyn Module>),
        (Dconf.get_name(), Box::new(Dconf) as Box<dyn Module>),
        (Debconf.get_name(), Box::new(Debconf) as Box<dyn Module>),
        (Debug.get_name(), Box::new(Debug) as Box<dyn Module>),
        (Dnf.get_name(), Box::new(Dnf) as Box<dyn Module>),
        (
            DockerContainer.get_name(),
            Box::new(DockerContainer) as Box<dyn Module>,
        ),
        (Fail.get_name(), Box::new(Fail) as Box<dyn Module>),
        (File.get_name(), Box::new(File) as Box<dyn Module>),
        (Firewalld.get_name(), Box::new(Firewalld) as Box<dyn Module>),
        (Find.get_name(), Box::new(Find) as Box<dyn Module>),
        (Gem.get_name(), Box::new(Gem) as Box<dyn Module>),
        (
            Filesystem.get_name(),
            Box::new(Filesystem) as Box<dyn Module>,
        ),
        (GetUrl.get_name(), Box::new(GetUrl) as Box<dyn Module>),
        (Git.get_name(), Box::new(Git) as Box<dyn Module>),
        (GpgKey.get_name(), Box::new(GpgKey) as Box<dyn Module>),
        (Group.get_name(), Box::new(Group) as Box<dyn Module>),
        (Hostname.get_name(), Box::new(Hostname) as Box<dyn Module>),
        (Include.get_name(), Box::new(Include) as Box<dyn Module>),
        (IniFile.get_name(), Box::new(IniFile) as Box<dyn Module>),
        (
            InterfacesFile.get_name(),
            Box::new(InterfacesFile) as Box<dyn Module>,
        ),
        (Iptables.get_name(), Box::new(Iptables) as Box<dyn Module>),
        (
            KernelBlacklist.get_name(),
            Box::new(KernelBlacklist) as Box<dyn Module>,
        ),
        (
            Lineinfile.get_name(),
            Box::new(Lineinfile) as Box<dyn Module>,
        ),
        (Locale.get_name(), Box::new(Locale) as Box<dyn Module>),
        (Lvg.get_name(), Box::new(Lvg) as Box<dyn Module>),
        (Lvol.get_name(), Box::new(Lvol) as Box<dyn Module>),
        (Meta.get_name(), Box::new(Meta) as Box<dyn Module>),
        (Modprobe.get_name(), Box::new(Modprobe) as Box<dyn Module>),
        (Mount.get_name(), Box::new(Mount) as Box<dyn Module>),
        (Nmcli.get_name(), Box::new(Nmcli) as Box<dyn Module>),
        (
            OpensslPrivatekey.get_name(),
            Box::new(OpensslPrivatekey) as Box<dyn Module>,
        ),
        (Pacman.get_name(), Box::new(Pacman) as Box<dyn Module>),
        (Parted.get_name(), Box::new(Parted) as Box<dyn Module>),
        (PamLimits.get_name(), Box::new(PamLimits) as Box<dyn Module>),
        (Reboot.get_name(), Box::new(Reboot) as Box<dyn Module>),
        (Script.get_name(), Box::new(Script) as Box<dyn Module>),
        (Seboolean.get_name(), Box::new(Seboolean) as Box<dyn Module>),
        (Service.get_name(), Box::new(Service) as Box<dyn Module>),
        (SetVars.get_name(), Box::new(SetVars) as Box<dyn Module>),
        (Setup.get_name(), Box::new(Setup) as Box<dyn Module>),
        (Slurp.get_name(), Box::new(Slurp) as Box<dyn Module>),
        (Stat.get_name(), Box::new(Stat) as Box<dyn Module>),
        (
            Synchronize.get_name(),
            Box::new(Synchronize) as Box<dyn Module>,
        ),
        (Sysctl.get_name(), Box::new(Sysctl) as Box<dyn Module>),
        (Systemd.get_name(), Box::new(Systemd) as Box<dyn Module>),
        (Template.get_name(), Box::new(Template) as Box<dyn Module>),
        (Timezone.get_name(), Box::new(Timezone) as Box<dyn Module>),
        (Trace.get_name(), Box::new(Trace) as Box<dyn Module>),
        (Unarchive.get_name(), Box::new(Unarchive) as Box<dyn Module>),
        (Uri.get_name(), Box::new(Uri) as Box<dyn Module>),
        (User.get_name(), Box::new(User) as Box<dyn Module>),
        (WaitFor.get_name(), Box::new(WaitFor) as Box<dyn Module>),
        (
            YumRepository.get_name(),
            Box::new(YumRepository) as Box<dyn Module>,
        ),
        (Zypper.get_name(), Box::new(Zypper) as Box<dyn Module>),
    ]
    .into_iter()
    .collect()
});

#[inline(always)]
pub fn is_module(module: &str) -> bool {
    MODULES.get(module).is_some()
}

#[inline(always)]
pub fn parse_params<P>(yaml: YamlValue) -> Result<P>
where
    for<'a> P: Deserialize<'a>,
{
    trace!("parse params: {yaml:?}");
    serde_norway::from_value(yaml).map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

#[inline(always)]
pub fn parse_if_json(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .flat_map(|s| serde_json::from_str(&s).unwrap_or_else(|_| vec![s]))
        .collect::<Vec<String>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_if_json() {
        let vec_string = parse_if_json(vec![
            r#"["yea", "foo", "boo"]"#.to_owned(),
            r#"["fuu", "buu"]"#.to_owned(),
            "yuu".to_owned(),
        ]);
        assert_eq!(
            vec_string,
            vec![
                "yea".to_owned(),
                "foo".to_owned(),
                "boo".to_owned(),
                "fuu".to_owned(),
                "buu".to_owned(),
                "yuu".to_owned()
            ]
        )
    }
}
