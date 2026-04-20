# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v2.19.2](https://github.com/rash-sh/rash/tree/v2.19.2) - 2026-04-07

### Fixed

- Correct import ordering in find.rs for rustfmt ([8f1ca6c](https://github.com/rash-sh/rash/commit/8f1ca6c07cf757d76fa399a98c06c7cd8e50dca8))

## [v2.19.1](https://github.com/rash-sh/rash/tree/v2.19.1) - 2026-03-26

### Added

- Add JSON output format support ([ea6ad0e](https://github.com/rash-sh/rash/commit/ea6ad0e6c9a005819368ca1e59fa5e7b5457cbc9))

### Fixed

- Sudo become method now properly handles output format propagation ([1cd52af](https://github.com/rash-sh/rash/commit/1cd52af1b8438eda2b78c87772310649edf1af4d))
- Properly propagate output format to sudo child processes ([4e07982](https://github.com/rash-sh/rash/commit/4e079829b61a3d1fab03459f97d248ef16c9e381))
- Make install-precommit.rh executable ([ff3f25e](https://github.com/rash-sh/rash/commit/ff3f25e95cc3de1a3a6e2bfc805b1230b19fa00b))
- Correct import order in logger.rs ([3f26ddb](https://github.com/rash-sh/rash/commit/3f26ddbccc08abfbb05c0ed1111f602d6d5367e9))

### Build

- deps: Update Rust crate uuid to v1.22.0 ([92e4a2d](https://github.com/rash-sh/rash/commit/92e4a2de24d7134a40697c98519a28cadff91f80))

## [v2.19.0](https://github.com/rash-sh/rash/tree/v2.19.0) - 2026-03-26

### Added

- Add become_method sudo support ([06c3fa3](https://github.com/rash-sh/rash/commit/06c3fa3b7176a5305803673d8d2d8407fd277133))
- Add sudo password support with --ask-become-pass ([7d4d0fa](https://github.com/rash-sh/rash/commit/7d4d0fa8eefff0b58a4ba7d7159611f369a1622b))

### Fixed

- Simplify sudo password handling and fix resource leak ([4f640c7](https://github.com/rash-sh/rash/commit/4f640c7d3b834346b82259c55383f99b137c9fb5))

### Documentation

- Add documentation for become_method and sudo password support ([c404134](https://github.com/rash-sh/rash/commit/c404134aff9bbce993f9edc657a1d1308e81ada7))

### Styling

- Fix clippy warnings instead of suppressing them ([4b04c80](https://github.com/rash-sh/rash/commit/4b04c8023c8ed9f0e098fe18a67c3c7079c3725c))

## [v2.18.3](https://github.com/rash-sh/rash/tree/v2.18.3) - 2026-03-25

### Fixed

- docker: Change LABEL format to `key=value` ([d42e8ba](https://github.com/rash-sh/rash/commit/d42e8bae2774d44eb07259007e602ed488b2a7ac))
- Change docker registry to ghcr.io ([31b5042](https://github.com/rash-sh/rash/commit/31b5042331ac8a95efb3b0dde3d742ddd2d2889a))

## [v2.18.2](https://github.com/rash-sh/rash/tree/v2.18.2) - 2026-03-25

### Fixed

- Skip become in check mode ([99c59ae](https://github.com/rash-sh/rash/commit/99c59aea06b16e0a4a66705b9279d03d291c4773))

## [v2.18.1](https://github.com/rash-sh/rash/tree/v2.18.1) - 2026-03-25

### Added

- book: Add llms.txt generation for llm discoverability ([0421b6d](https://github.com/rash-sh/rash/commit/0421b6d8fabe31859e40a93aff223f76edac13a2))

### Documentation

- Fix modules link in getting started ([2bcc2cc](https://github.com/rash-sh/rash/commit/2bcc2cce95b7476d982fd4d36c54a067f52226f8))
- Add release skill for opencode ([f30c1c6](https://github.com/rash-sh/rash/commit/f30c1c6a8c0557d96c1383abce8251bd38949e40))

### Build

- deps: Update Rust crate clap to v4.6.0 ([80e38b7](https://github.com/rash-sh/rash/commit/80e38b7ebb23be37a0d690422ffc003119a51cca))
- deps: Update Rust crate console to v0.16.3 ([183319e](https://github.com/rash-sh/rash/commit/183319e83222d381dda99ff39483b1ae55ff95cf))
- deps: Update Rust crate serde_with to v3.18.0 ([26d1200](https://github.com/rash-sh/rash/commit/26d12008b05b93ca2aeb412c759b1541ef87a33c))
- deps: Update Rust crate minijinja to v2.18.0 ([3731ee0](https://github.com/rash-sh/rash/commit/3731ee0fbacb2c10651ed3667c94b1ceb289e500))
- deps: Update Rust crate vaultrs to 0.8 ([56b85b5](https://github.com/rash-sh/rash/commit/56b85b51a004c7150604fd92e79625b352b474c6))
- deps: Update Rust crate zip to v8.3.0 ([ceb5e25](https://github.com/rash-sh/rash/commit/ceb5e25ec497f8ccea0e0be353735bae43fc9d70))
- deps: Update Rust crate tar to v0.4.45 ([db18edf](https://github.com/rash-sh/rash/commit/db18edf6cfde940a1e2f62838307353debe6a38d))
- deps: Update Rust crate redis to v1.1.0 ([876f89d](https://github.com/rash-sh/rash/commit/876f89d0aa742095a883add7c4bc2d7cf8d3576e))
- deps: Update Rust crate zip to v8.3.1 ([13d0896](https://github.com/rash-sh/rash/commit/13d0896a83890ee28a13fffa9e4934a38ecefb52))
- deps: Update Rust crate zip to v8.4.0 ([2d3091f](https://github.com/rash-sh/rash/commit/2d3091f5168629c7cda5a2202f5273e6adbfcfe3))
- deps: Update Rust crate env_logger to v0.11.10 ([e9c3b6b](https://github.com/rash-sh/rash/commit/e9c3b6b93b70e68c5cedd88bf1e28553592ca841))
- deps: Bump rustls-webpki from 0.103.9 to 0.103.10 ([9e65720](https://github.com/rash-sh/rash/commit/9e657209ae3e0539be983e9442214dcd387c1aa6))

### Refactor

- book: Improve llms.txt generation maintainability ([3b25023](https://github.com/rash-sh/rash/commit/3b25023f9ae99a85c97c3ca8b74a1d5872b2b479))

## [v2.18.0](https://github.com/rash-sh/rash/tree/v2.18.0) - 2026-03-11

### Added

- copy: Add directory support for src and dest ([e5d575b](https://github.com/rash-sh/rash/commit/e5d575b5bea5a44fdd4697ddb75c3b35465fd107))
- docker_container: Add restarted state and detach parameter ([759982d](https://github.com/rash-sh/rash/commit/759982dc1604294416697c0ca379e51d1e139ea7))
- module: Add stat module for file metadata ([704fc5d](https://github.com/rash-sh/rash/commit/704fc5d3586b1419a60a64e28b6dd5b8c6e086b7))
- module: Add fail module for explicit failure ([3b1291e](https://github.com/rash-sh/rash/commit/3b1291e38c5f1cebfc3d8f91249ae6248fe7be7a))
- module: Add ini_file module for INI configuration management ([1423dd6](https://github.com/rash-sh/rash/commit/1423dd6636274bbb3d7ecf51f6610a5edd3fae1f))
- module: Add slurp module for reading files ([7b1749a](https://github.com/rash-sh/rash/commit/7b1749a2e9087b281c8a88dde45d0d262479c523))
- module: Add archive and unarchive modules for tar/zip extraction ([49d14a9](https://github.com/rash-sh/rash/commit/49d14a96663d5b17e20745e9697fb2cb2763cfed))
- module: Add trace module for ebpf/bpftrace integration ([a3babcc](https://github.com/rash-sh/rash/commit/a3babccc5888fa28b15143b6b181b037dc0de5a1))
- module: Add apk module for alpine package management ([61eb472](https://github.com/rash-sh/rash/commit/61eb472c1e95bad942d70f40e5fe57427f464f37))
- module: Add script module for running script files ([62b4ed1](https://github.com/rash-sh/rash/commit/62b4ed10148e5047cf12ce88f4ff27511d637cc5))
- module: Add integration tests for timezone module ([537a9cf](https://github.com/rash-sh/rash/commit/537a9cf986acc850f79ba34e0153fef00aa3a9c4))
- module: Add synchronize module for rsync operations ([2845f4b](https://github.com/rash-sh/rash/commit/2845f4bfa681b49c24f12e02501ce8e281d80a32))
- module: Add reboot module for system restart management ([6f2d90d](https://github.com/rash-sh/rash/commit/6f2d90d9fc78a655a340f412efde28482df705f6))
- module: Add service module for systemd/init service management ([8742587](https://github.com/rash-sh/rash/commit/874258748a788064da1d066427e4953508fd83ae))
- module: Add pam_limits module for managing Linux PAM limits ([a450239](https://github.com/rash-sh/rash/commit/a4502393013ffe5d62a8173585298976ae99e4d1))
- module: Add interfaces_file module for network interface configuration ([aac0e79](https://github.com/rash-sh/rash/commit/aac0e79a16d52c66559795d2c8a450ec85854b1e))
- module: Add seboolean module for SELinux boolean management ([803ac39](https://github.com/rash-sh/rash/commit/803ac39faf15df90c70380aa0353ebf1af51d7d5))
- module: Add apt module for debian and ubuntu package management ([779e377](https://github.com/rash-sh/rash/commit/779e3771935928f116ea1ae92b8e505847ddcdd2))
- module: Add dnf module for Fedora/RHEL package management ([41e5450](https://github.com/rash-sh/rash/commit/41e54502bb867bfb894d3bf1a214fe925e002ba8))
- module: Add parted module for disk partition management ([11d5635](https://github.com/rash-sh/rash/commit/11d5635ab2c7edc5538913bcce8a58a6ded8498e))
- module: Add lvg module for LVM volume group management ([00a373a](https://github.com/rash-sh/rash/commit/00a373a1b2334c346631907b5b0515db12f9b65e))
- module: Add gpg_key module for GPG key management ([898b0d6](https://github.com/rash-sh/rash/commit/898b0d6fa1b23fbacca3af4a7af1dbce24eebaf8))
- module: Add firewalld module for firewall management ([cc5bea9](https://github.com/rash-sh/rash/commit/cc5bea9fe69b83c42589a232cfd5bb17e26f953f))
- module: Add nmcli module for networkmanager configuration ([4f29c77](https://github.com/rash-sh/rash/commit/4f29c775771cc3979b3068d1bb2be32274c460c6))
- module: Add at module for one-time scheduled jobs ([316e1c1](https://github.com/rash-sh/rash/commit/316e1c125d23f4cac1329c80c137ef894927d57a))
- module: Add filesystem module for filesystem creation ([d5b7b4c](https://github.com/rash-sh/rash/commit/d5b7b4cb3c5a1a6e3ba86c5db102bd7b7c8366bb))
- module: Add alternatives module for managing system alternatives ([d307107](https://github.com/rash-sh/rash/commit/d307107ab51cffb3b76a3ff8c6649a880a224eef))
- module: Add zypper module for openSUSE/SLES package management ([b371643](https://github.com/rash-sh/rash/commit/b3716434b74bde2cd5dd200108c7cdd4d565fa1c))
- module: Add timezone example file ([e814e18](https://github.com/rash-sh/rash/commit/e814e18657afb512798cd9ee76ea0223b4802a55))
- module: Add modprobe module for kernel module management ([6b9f020](https://github.com/rash-sh/rash/commit/6b9f020b31701c1e231fc4713550218163283521))
- module: Add iptables module for firewall rule management ([e55794b](https://github.com/rash-sh/rash/commit/e55794ba5eac81fbd0797e907cbb0bb5f6aff121))
- module: Add lvol module for lvm logical volume management ([65a06df](https://github.com/rash-sh/rash/commit/65a06dfa456e2b6ba55928b5d95bcc5d5400afe6))
- module: Add debconf module for debian package configuration ([17a3608](https://github.com/rash-sh/rash/commit/17a3608317b8f85bbe05733ce42d675c7f3d4c23))
- module: Add debconf module for Debian package configuration ([63bb823](https://github.com/rash-sh/rash/commit/63bb823ad3360c928e7492644c7a317cb7acba04))
- module: Add modprobe module for kernel module management ([33fcc0c](https://github.com/rash-sh/rash/commit/33fcc0caa22f4b4ceb7202dccd3da6dc520e080a))
- module: Add locale module for system locale management ([82636d6](https://github.com/rash-sh/rash/commit/82636d6ec68f2baaa0c6fc58a9b53bbf7e08816c))
- module: Add openssl_privatekey module for SSL private key generation ([2585567](https://github.com/rash-sh/rash/commit/2585567ed3cca89b9b4f205b8dc0d342fca8e022))
- module: Add apt_repository module for APT repository management ([479eca2](https://github.com/rash-sh/rash/commit/479eca25c3386f5f001a3640b39118594bddcbe5))
- module: Add yum_repository module for YUM/DNF repository management ([283c633](https://github.com/rash-sh/rash/commit/283c6338b9516e80a46c945035093432012d6c48))
- module: Add docker_container module for container management ([ff43cbb](https://github.com/rash-sh/rash/commit/ff43cbb8d49cc5227badac25b3f079242c03554b))
- module: Add gem module for Ruby package management ([a385ed5](https://github.com/rash-sh/rash/commit/a385ed53112f44f42fe8b0ffe9b94b5db9044582))
- module: Add kernel_blacklist module for kernel module blacklist ([c66ac52](https://github.com/rash-sh/rash/commit/c66ac52e25000f2fd6dac1089b122690ca26f722))
- module: Add selinux module for SELinux configuration ([511254f](https://github.com/rash-sh/rash/commit/511254fa7d051b8d27075333ea595bccc04b2a4f))
- module: Add cargo module for Rust package management ([0ebffb0](https://github.com/rash-sh/rash/commit/0ebffb0da58d58973393e3c11207e96db99988b2))
- module: Add ping module for connectivity testing ([2637260](https://github.com/rash-sh/rash/commit/2637260449a8a0c6f6e73a2f07abad1c48c1de05))
- module: Add logrotate module for log rotation configuration ([06d2a7d](https://github.com/rash-sh/rash/commit/06d2a7dceb01f4e3392a5e78b2102724f6a14fc1))
- module: Add dmsetup module for device mapper management ([2e38832](https://github.com/rash-sh/rash/commit/2e3883246a31ce2b31dfcdbb1790b900ce4f449c))
- module: Add chroot module for chroot commands ([3725f13](https://github.com/rash-sh/rash/commit/3725f13b28b18169ac8ad26b8ab13be99b0983de))
- module: Add zfs module for ZFS dataset management ([309cd06](https://github.com/rash-sh/rash/commit/309cd06381128341134ee2b6328917b6ca8aac95))
- module: Add debootstrap module for Debian/Ubuntu installation ([252051a](https://github.com/rash-sh/rash/commit/252051abb61f155ec0a81d6404b273129684ad0e))
- module: Add initramfs module for initramfs management ([068d1fe](https://github.com/rash-sh/rash/commit/068d1fe4ff5fd24b69ba12cdba2b5006f09e643f))
- module: Add blkdiscard module for SSD secure erase ([ae9f616](https://github.com/rash-sh/rash/commit/ae9f616144987d1b3cf4b294f3d11232d98409db))
- module: Add npm module for Node.js package management ([8312eff](https://github.com/rash-sh/rash/commit/8312effd80b7a41fe9f14fce18e2cfa883737366))
- module: Add mdadm module for software RAID management ([0231d62](https://github.com/rash-sh/rash/commit/0231d62a47d481a59133fd3eb26b93d56577fffc))
- module: Add composer module for PHP dependency management ([d302f8b](https://github.com/rash-sh/rash/commit/d302f8b0e64ec5d1122c0890580d451c366ce556))
- module: Add postgresql_db module for PostgreSQL database management ([543b42f](https://github.com/rash-sh/rash/commit/543b42fcf0aba6e583d942c19716e829d47f86d5))
- module: Add zpool module for ZFS storage pool management ([d0efa12](https://github.com/rash-sh/rash/commit/d0efa1206e1d4a95a6789bd369ec9219c85a64b2))
- module: Add json_file module for JSON file manipulation ([06553d0](https://github.com/rash-sh/rash/commit/06553d0e8b1bfdb1f56fcf243fd1847a1a19215e))
- module: Add make module for build automation ([27ef2f6](https://github.com/rash-sh/rash/commit/27ef2f6ef539d6bc2911d49f4cef860f72fcc4dc))
- module: Add openssl_csr module for CSR generation ([5768feb](https://github.com/rash-sh/rash/commit/5768feb4da3c424768dfc99f7093b9a753a6fd9c))
- module: Add openssl_certificate module for certificate management ([8ba5e7e](https://github.com/rash-sh/rash/commit/8ba5e7e293b18b5a53d3245766848e535ed8b5f8))
- module: Add dynamic module infrastructure ([cf05bae](https://github.com/rash-sh/rash/commit/cf05baefcaead4c41ca5cac29e2a552d8e224734))
- module: Add netplan module for Ubuntu network configuration ([867b5df](https://github.com/rash-sh/rash/commit/867b5df21ec49783eae94ed8ef15227d73672acd))
- module: Add generic package module for cross-distro package management ([dd035d3](https://github.com/rash-sh/rash/commit/dd035d31373829f925136102e59aa032af50b72f))
- module: Add pause module for execution control ([25be9a0](https://github.com/rash-sh/rash/commit/25be9a0bee8a7078e0f84b8ff803878821d27015))
- modules: Add git module for repository operations ([b806e4a](https://github.com/rash-sh/rash/commit/b806e4ab04f9b30102ccb5281d5b877008472c4e))
- modules: Add timezone module for time configuration ([894c572](https://github.com/rash-sh/rash/commit/894c572c52651c206ee7abfaf19b09d7fef5ae14))
- modules: Add hostname module for system hostname management ([2de0e5c](https://github.com/rash-sh/rash/commit/2de0e5c8c692477c93d17c1e4914e44abc43ad6a))
- modules: Add authorized_key module for SSH key management ([cd5a387](https://github.com/rash-sh/rash/commit/cd5a38761e962df6dbe90b773a65aff508cf2659))
- modules: Add sysctl module for kernel parameters ([123bfc8](https://github.com/rash-sh/rash/commit/123bfc8b60bbb6f5c67fa48d3f1066571a325555))
- task: Add retry/until support for tasks ([91f98e1](https://github.com/rash-sh/rash/commit/91f98e1d4945f4ca5dbbcf8ac8ba4561341a10eb))
- Add wait_for module to wait for tcp port availability ([adad234](https://github.com/rash-sh/rash/commit/adad2346cbbdded37f9bbc519069379e0fff6d7d))
- Add mount module for filesystem mounts ([20f94d9](https://github.com/rash-sh/rash/commit/20f94d966a9c36bccf2f5ce300ac5ff9c9cfc228))
- Add async task execution support ([e02e0ad](https://github.com/rash-sh/rash/commit/e02e0ad6910bfb599ea6dc1b0a02ca8bacc8b0e6))
- Add wipefs module for filesystem signature wiping ([f580dd4](https://github.com/rash-sh/rash/commit/f580dd4475861d1e0e0514c682d4c23cb7d7e2a6))
- Add grub module for bootloader management ([5334da4](https://github.com/rash-sh/rash/commit/5334da4645345bf0466c9315346e4e9b30c8944d))
- Add docker_image module for Docker image management ([8dd1781](https://github.com/rash-sh/rash/commit/8dd17816586f08327fd8f2e75a22a75e8b409b27))
- Add known_hosts module for SSH host key management ([07d1bb1](https://github.com/rash-sh/rash/commit/07d1bb18c928860bd4e522b2dc73f540e4557254))
- Add integration tests for apt and pip modules ([8eb8f78](https://github.com/rash-sh/rash/commit/8eb8f7835d10c3ebcfbe12733394ba522be68029))
- Add rash.check_mode builtin variable ([16429b6](https://github.com/rash-sh/rash/commit/16429b683b774fb146c6426954bf08fa8cb8d2e4))

### Fixed

- book: Add meta module docs and correct weight calculation ([91b49fd](https://github.com/rash-sh/rash/commit/91b49fdf6f69f74457aa016a3e4d53d5f894935b))
- ci: Remove DCO config ([f5742ac](https://github.com/rash-sh/rash/commit/f5742ac0ab9d453352ce6fce3ca1dfee8863cc46))
- command: Support check mode by skipping execution ([490d715](https://github.com/rash-sh/rash/commit/490d715c8ca8f80aaf61c540f4c011f547a89cb6))
- core: Use matching ids for passwd/group files in user tests ([675e008](https://github.com/rash-sh/rash/commit/675e0080702c0276f1855646be24fd41faf301e1))
- core: Use tempfile for truly unique test files ([d84bf9b](https://github.com/rash-sh/rash/commit/d84bf9bf9c043ee212c48e86af11f1dfe71db754))
- core: Check for specific timezone files in tests ([dbc2ffc](https://github.com/rash-sh/rash/commit/dbc2ffc2fb56a5f11cfd8a16ab8615d73fc0b1ac))
- module: Add sgdisk module for GPT partition management ([abf05e7](https://github.com/rash-sh/rash/commit/abf05e70d8765e83e11edcaef41c3a5cd18f40cd))
- module: Add mysql_db module for MySQL/MariaDB database management ([6c85966](https://github.com/rash-sh/rash/commit/6c8596612473655f4fe8813f213a4e7adc111d66))
- module: Add lbu module for Alpine Local Backup ([85e54d3](https://github.com/rash-sh/rash/commit/85e54d3d538690d1d789c84a5f24d7d47cd7df82))
- module: Add expect module for interactive command automation ([d5ceee1](https://github.com/rash-sh/rash/commit/d5ceee14d986cffd47a8c716ec5c653c071137b3))
- module: Add pip module for Python package management ([677355e](https://github.com/rash-sh/rash/commit/677355ee9d0443cbd4165fac958a9f3e25786e3e))
- module: Rebase xml on master to resolve conflicts ([acc42f2](https://github.com/rash-sh/rash/commit/acc42f2f4c7f357829541907c85d219d40df1d2d))
- module: Use time::Duration instead of std::time::Duration ([bf14033](https://github.com/rash-sh/rash/commit/bf14033d9e4bbb40b105b6b062de93a687d9dd94))
- module: Register netplan module in mod.rs ([d097292](https://github.com/rash-sh/rash/commit/d09729211830985308e339fcfd7948fc48093de6))
- modules: Use compatible syntax in ini_file module ([e50916c](https://github.com/rash-sh/rash/commit/e50916c12527a94d3bb0a05905c9edba3c37782a))
- modules: Adopt master formatting for ini_file ([a4d4540](https://github.com/rash-sh/rash/commit/a4d454009359ebe4cec7514d15766bde857c6d09))
- tests: Skip timezone tests when zoneinfo unavailable ([9bda56f](https://github.com/rash-sh/rash/commit/9bda56fa8285713a6880a92acfdfb84ae5566aeb))
- Use u8 buffer for getpwuid_r/getgrgid_r to support aarch64 ([704f0d1](https://github.com/rash-sh/rash/commit/704f0d15d135114c23e0f46b7e890e2f5943973b))
- Resolve rebase conflicts and fix compilation errors ([84a288c](https://github.com/rash-sh/rash/commit/84a288c6bb7b5b84d2e9bd1020f17621fd56bfc2))
- Resolve rebase conflicts and format code ([feb61bb](https://github.com/rash-sh/rash/commit/feb61bbfd12df3ab8d7b1c8ac0443203868127b7))
- Add shebang to ini_file example ([1a2c890](https://github.com/rash-sh/rash/commit/1a2c8905827a754040e31a383d86d9b3f413796f))
- Make ini_file example executable ([b31b064](https://github.com/rash-sh/rash/commit/b31b06450683ecd5bdec43ac1c362c6fa9ca56c2))
- Skip rsync test when rsync is not available ([f71c6d0](https://github.com/rash-sh/rash/commit/f71c6d047ee577160a05e0b012633ee3f2d09db2))
- Exclude apt.rh from test-examples ([af74ac1](https://github.com/rash-sh/rash/commit/af74ac1674d502802cf3712a3054d889825a1c01))
- Make apt example executable ([c6f37a3](https://github.com/rash-sh/rash/commit/c6f37a3f545df137d0623132bc447327b847cbca))
- Exclude timezone.rh from test-examples (requires root) ([c4e2193](https://github.com/rash-sh/rash/commit/c4e2193db0474cbfec8d8a2e97251b4b9a583992))
- Remove duplicate Debconf and Modprobe entries in mod.rs ([c215264](https://github.com/rash-sh/rash/commit/c215264d14e57258838f6db92dce9595f2866896))
- Exclude openssl_privatekey.rs from private key detection ([9f000ba](https://github.com/rash-sh/rash/commit/9f000ba9a40a5594e2e90cbc4e86a8050b84d7bf))
- Make blkdiscard example executable ([e4e8cc8](https://github.com/rash-sh/rash/commit/e4e8cc8709452bace91b08a629840c77794bd381))
- Exclude blkdiscard and mdadm examples from test-examples ([c57d538](https://github.com/rash-sh/rash/commit/c57d5389d474e027815dbe9a475a4d0c06b11423))
- Make mdadm example executable ([ef394c8](https://github.com/rash-sh/rash/commit/ef394c8825979e618acc65ae3e853d792a2af082))
- Update yum_repository module with improved docs and error handling ([bb92820](https://github.com/rash-sh/rash/commit/bb928202d4e30f7de06ee8f7466eff2fb7e379d4))
- Update java_keystore module and resolve conflicts ([4bb6450](https://github.com/rash-sh/rash/commit/4bb64501c5c511fbba5fcff678a64edaa98a8508))
- Update redis module and resolve conflicts ([f4379aa](https://github.com/rash-sh/rash/commit/f4379aa64f9fc754969684e3179df2b58519939e))
- Update openssl_certificate example to match module params ([d816d6b](https://github.com/rash-sh/rash/commit/d816d6b588ac3e75a15df1fa7ed0e86e144906eb))
- Exclude openssl_certificate.rh from test-examples ([550af49](https://github.com/rash-sh/rash/commit/550af49c48973affba01897e58483944ebddd9bc))
- Remove trailing whitespace in docs ([9523bd1](https://github.com/rash-sh/rash/commit/9523bd1e51b61fc53ba4093b27ce7604c46f0b75))
- Update test for minijinja 2.17.0 compatibility ([db9211d](https://github.com/rash-sh/rash/commit/db9211d8d5c9d7e8f597329305851f6819b38e4f))

### Documentation

- Add ini_file module example ([085ce47](https://github.com/rash-sh/rash/commit/085ce470dbd3c4e1530e6984ee52ab96f1d988bf))
- Add VM dev tooling modules analysis for #1340 ([ab861b8](https://github.com/rash-sh/rash/commit/ab861b81523ebb0643017a83f4c75bc88a6f7d5c))
- Update analysis with created issue numbers ([235d73c](https://github.com/rash-sh/rash/commit/235d73c2409eefaeef75ef1e89aec83afe4cfbf2))
- Remove snap from VM dev tooling recommendations ([c99f4f0](https://github.com/rash-sh/rash/commit/c99f4f0cbaa8c9ce0549b7c506c4ece2d01d116c))

### Build

- ci: Automerge patch and minor requests ([81c0a3d](https://github.com/rash-sh/rash/commit/81c0a3d673f3ef1e1655d40cdfe1dea39cb332d6))
- deps: Update Rust crate proc-macro2 to v1.0.106 ([bbacec3](https://github.com/rash-sh/rash/commit/bbacec3bd63cea49483a0250b25ab3c09377b490))
- deps: Update Rust crate quote to v1.0.44 ([2b7d9e6](https://github.com/rash-sh/rash/commit/2b7d9e62d17391feaa1781119bb9b18004fc6d99))
- deps: Update rust Docker tag to v1.93.0 ([debacb8](https://github.com/rash-sh/rash/commit/debacb8214c7bf9826186b50cb5a720a1fd2a6e3))
- deps: Update nix to 0.31 ([8d4296d](https://github.com/rash-sh/rash/commit/8d4296db3f0200df9ca4f06d5a2baf82188e3101))
- deps: Update Rust crate minijinja to v2.15.1 ([23ea372](https://github.com/rash-sh/rash/commit/23ea372e29627712a9addb5aab696dafbd9bb12e))
- deps: Update Rust crate clap to v4.5.55 ([fe3a115](https://github.com/rash-sh/rash/commit/fe3a1155a6519babee00882c56ffae5bcd1b8a27))
- deps: Update Rust crate clap to v4.5.56 ([94a9553](https://github.com/rash-sh/rash/commit/94a95532c9496af7ff086c38c7255dcc7a899d9b))
- deps: Update Rust crate schemars to v1.2.1 ([50e1507](https://github.com/rash-sh/rash/commit/50e15072477ffccd46a4703cd6568f603ccb4ceb))
- deps: Update Rust crate regex to v1.12.3 ([f85b360](https://github.com/rash-sh/rash/commit/f85b360617426ae79af8771cc616d551d2f237fa))
- deps: Update Rust crate clap to v4.5.57 ([7b6880e](https://github.com/rash-sh/rash/commit/7b6880e74c9bc47db523590977f1244b71aefb61))
- deps: Update Rust crate ipc-channel to 0.21.0 ([5f29ab4](https://github.com/rash-sh/rash/commit/5f29ab49ddab9f837c3d77abfd80f4562ca9a3d8))
- deps: Update Rust crate criterion to v0.8.2 ([3d15fb7](https://github.com/rash-sh/rash/commit/3d15fb789e9a72d310932e3e802555f0040c34b6))
- deps: Update Rust crate reqwest to v0.13.2 ([361ab2e](https://github.com/rash-sh/rash/commit/361ab2eba74552478a30beccaa317d216bc177c2))
- deps: Bump bytes from 1.10.1 to 1.11.1 ([7363480](https://github.com/rash-sh/rash/commit/7363480ac1776338673c40f66caf6606a57e7be2))
- deps: Update Rust crate tempfile to v3.25.0 ([8d0233a](https://github.com/rash-sh/rash/commit/8d0233a8ec54ef5ebd6e7c6b06b3bd643b4649f8))
- deps: Update Rust crate clap to v4.5.58 ([708c62f](https://github.com/rash-sh/rash/commit/708c62f1ef907f6ccedd0d378226caad42957b42))
- deps: Update Rust crate env_logger to v0.11.9 ([d289997](https://github.com/rash-sh/rash/commit/d289997f8e96ecd0fe67efd4f56d3ca1f4fe24ae))
- deps: Update Rust crate syn to v2.0.115 ([de6cbf4](https://github.com/rash-sh/rash/commit/de6cbf401efc60a6d245ad440a69fb23b754a1f0))
- deps: Update rust Docker tag to v1.93.1 ([57fa133](https://github.com/rash-sh/rash/commit/57fa1333b9de804f0758c7c37713fdd36974a4b4))
- deps: Bump time from 0.3.43 to 0.3.47 ([307073b](https://github.com/rash-sh/rash/commit/307073ba246bed93e686d091b58ac073071bae72))
- deps: Update Rust crate syn to v2.0.116 ([299449c](https://github.com/rash-sh/rash/commit/299449c890fd2534bee00d2482ac890b251edeb0))
- deps: Update Rust crate clap to v4.5.59 ([09b17ee](https://github.com/rash-sh/rash/commit/09b17ee555162c6734f7debd4a29b367a0510bfe))
- deps: Update Rust crate bzip2 to 0.6 ([df65728](https://github.com/rash-sh/rash/commit/df65728ec3016acb18a42d93ae96552f630117b0))
- deps: Update Rust crate zip to v8 ([4288e6a](https://github.com/rash-sh/rash/commit/4288e6add3bfc6734b2026d57415c07232b95e2c))
- deps: Update Rust crate rand to 0.10 ([738b355](https://github.com/rash-sh/rash/commit/738b35505d9aeae64dfe5a001eb2125f573d2eef))
- deps: Update Rust crate clap to v4.5.60 ([cae4d66](https://github.com/rash-sh/rash/commit/cae4d6683c48525b1503d7aef468d45abb6c692a))
- deps: Update Rust crate syn to v2.0.117 ([b41fb2c](https://github.com/rash-sh/rash/commit/b41fb2cc183543572c5da532c88f4431122a6fc5))
- deps: Update Rust crate minijinja to v2.16.0 ([418ee06](https://github.com/rash-sh/rash/commit/418ee0662d87b45598a7f3c86b30a4c4d4761d7a))
- deps: Update Rust crate clap to v4.5.60 ([16bca3e](https://github.com/rash-sh/rash/commit/16bca3ef1ea3d2d651799701d075a4fedcde2ab7))
- deps: Update Rust crate syn to v2.0.117 ([eadc889](https://github.com/rash-sh/rash/commit/eadc889255147503841dcab550ebe13300c531c9))
- deps: Update Rust crate minijinja to v2.16.0 ([2b482dd](https://github.com/rash-sh/rash/commit/2b482dd29d995aea294a1cf10d3530f294794313))
- deps: Update strum monorepo to 0.28.0 ([7d3447c](https://github.com/rash-sh/rash/commit/7d3447c2a98f7c67f6771da0560e232c6f825b5d))
- deps: Update Rust crate minijinja to v2.16.0 ([082830d](https://github.com/rash-sh/rash/commit/082830d32ed8f2a4b8799e2fc79acce674230f88))
- deps: Update Rust crate chrono to v0.4.44 ([9d69592](https://github.com/rash-sh/rash/commit/9d69592a2b77ee9b394bc715d800f476e400aa52))
- deps: Update Rust crate serde_with to v3.17.0 ([0e72726](https://github.com/rash-sh/rash/commit/0e7272635d41db27373562c6fa248a7db584901b))
- deps: Update Rust crate rcgen to 0.14 ([ee62ec8](https://github.com/rash-sh/rash/commit/ee62ec85ab89751446fb03d0198d24377036cc64))
- deps: Update Rust crate redis to v1 ([ef233f4](https://github.com/rash-sh/rash/commit/ef233f4b3edf80c59c19307572ee6cf1e26a9f01))
- deps: Update Rust crate quick-xml to 0.39 ([d327162](https://github.com/rash-sh/rash/commit/d3271627f2bed779513154aa2fe25f72d5975a35))
- deps: Update GitHub Artifact Actions ([6f6efd3](https://github.com/rash-sh/rash/commit/6f6efd3a141cb3e7a991416b70e6ac00338a3d2e))
- deps: Update Rust crate nix to v0.31.2 ([6bcf3aa](https://github.com/rash-sh/rash/commit/6bcf3aab7032e320cc936e7ac14a763c808605c4))
- deps: Update Rust crate tempfile to v3.26.0 ([521d792](https://github.com/rash-sh/rash/commit/521d792daa60815ff9b9aecc57298c3fd8dbd141))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v42.95.1 ([92110c6](https://github.com/rash-sh/rash/commit/92110c6c4399e25b65ee2c4c4e29af0619d4eda1))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v43 ([f0d04db](https://github.com/rash-sh/rash/commit/f0d04db10ae02c0fd30ad59035de2440f762fa53))
- deps: Update Rust crate zip to v8.2.0 ([f0d9149](https://github.com/rash-sh/rash/commit/f0d91495cd5d002d4e8941b323596c78ba44c96e))
- deps: Update Rust crate tokio to v1.50.0 ([5f8242a](https://github.com/rash-sh/rash/commit/5f8242aa512af0def712f289957eeb89017f01ff))
- deps: Update crazy-max/ghaction-upx action to v4 ([3965e74](https://github.com/rash-sh/rash/commit/3965e747dd40c6292c563a617b10d2fb26dfdaf1))
- deps: Update Rust crate quote to v1.0.45 ([98a2657](https://github.com/rash-sh/rash/commit/98a2657f5db37550734c12e51657bbfb241d9c62))
- deps: Update docker/login-action action to v4 ([0dad8bf](https://github.com/rash-sh/rash/commit/0dad8bf3664fd2363760dfb74dd49e586ff8fbb8))
- deps: Update docker/setup-qemu-action action to v4 ([18a99ed](https://github.com/rash-sh/rash/commit/18a99ed43b404a357209821ee8c22dfc65c4515f))
- deps: Update docker/setup-buildx-action action to v4 ([603ce7a](https://github.com/rash-sh/rash/commit/603ce7a94380f5de5c1e8ebf820571ec4df7a13a))
- deps: Update rust Docker tag to v1.94.0 ([24ab876](https://github.com/rash-sh/rash/commit/24ab8769d841347a0bf95d5e0fc40785b95e3344))
- deps: Update Rust crate libc to v0.2.183 ([ad28cff](https://github.com/rash-sh/rash/commit/ad28cfff61ab59c77feaa9aa116b8d988c0d7685))
- deps: Update Rust crate redis to v1.0.5 ([491d895](https://github.com/rash-sh/rash/commit/491d895d4063a71e3c4c0ef8452a64829514abfc))
- deps: Update Rust crate minijinja to v2.17.0 ([defac80](https://github.com/rash-sh/rash/commit/defac80003ff6288a1566f8c99f06df3e137347c))
- deps: Update Rust crate minijinja to v2.17.1 ([0073c0a](https://github.com/rash-sh/rash/commit/0073c0a8d30c42a639d7ec8b287de4bca383c67a))
- deps: Update Rust crate tempfile to v3.27.0 ([d5d5207](https://github.com/rash-sh/rash/commit/d5d5207408a48f328c5cec25eeb4f650dee31886))
- deps: Bump quinn-proto from 0.11.13 to 0.11.14 ([356f735](https://github.com/rash-sh/rash/commit/356f735d6630d122b60614297ea8a83c01c0409a))

### Refactor

- Merge origin/master and fix conflicts ([fc90c51](https://github.com/rash-sh/rash/commit/fc90c5106554568d36f073e3b88399b863e0a05f))

### Styling

- Fix formatting in stat.rs after rebase ([48cba90](https://github.com/rash-sh/rash/commit/48cba9053e79034aa316cd17deaea8e45a0bfc69))
- Fix formatting in synchronize module ([28ffd08](https://github.com/rash-sh/rash/commit/28ffd08e6da580e39fa3bd718278f6e9f5f44b6f))
- Fix formatting ([69dd09a](https://github.com/rash-sh/rash/commit/69dd09af028447f132d561a60dd4195af6f7dcad))
- Fix import ordering in command module ([e81ca3d](https://github.com/rash-sh/rash/commit/e81ca3d620d82418fe9f9016fba7e1a7d906ca91))

### Testing

- module: Add integration tests for cargo module ([e9787a1](https://github.com/rash-sh/rash/commit/e9787a17b5184849fe941999a4720f377abfdfb4))
- Add integration tests for git module ([ea1173b](https://github.com/rash-sh/rash/commit/ea1173bb54bc9c493b57effdf55f1e61dae30cb7))

### Merge

- Resolve conflicts with upstream/master ([dcd9067](https://github.com/rash-sh/rash/commit/dcd9067f905e46e6c0292aba5a37e43248981e41))

## [v2.17.8](https://github.com/rash-sh/rash/tree/v2.17.8) - 2026-01-20

### Added

- ci: Add FreeBSD release targets ([a4906d5](https://github.com/rash-sh/rash/commit/a4906d5482e478bad5d61d6333744769c0c713d4))

## [v2.17.7](https://github.com/rash-sh/rash/tree/v2.17.7) - 2026-01-20

### Fixed

- deps: Fix reqwest feature name for v0.13 compatibility ([2bc2393](https://github.com/rash-sh/rash/commit/2bc2393f8f88f1e4f79f493bebdbc28bb68a4ccc))

### Build

- deps: Update Rust crate reqwest to 0.13 ([95f3be9](https://github.com/rash-sh/rash/commit/95f3be9e82bb8424a3ebec48081a479fac3ba92f))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v42.84.0 ([f5b00f7](https://github.com/rash-sh/rash/commit/f5b00f7ba7d9aa5d1e397e2085941569c4e4de99))
- deps: Update Rust crate prs-lib to v0.5.7 ([0bd028b](https://github.com/rash-sh/rash/commit/0bd028b75e0607d8006f3d8dc4e043e6048a5659))

## [v2.17.6](https://github.com/rash-sh/rash/tree/v2.17.6) - 2026-01-16

### Fixed

- task: Skip omit values in loop iteration ([280a763](https://github.com/rash-sh/rash/commit/280a763fad4ca5a998e843a28b08dca4a163656d))

### Build

- deps: Update Rust crate schemars to v1.2.0 ([dfe3cbb](https://github.com/rash-sh/rash/commit/dfe3cbbb74aee0309611b8610bc59eb24bee9dbf))
- deps: Update Rust crate serde_json to v1.0.148 ([77b00d1](https://github.com/rash-sh/rash/commit/77b00d14ea75cfdf76a8f8588efe59bd0815919b))
- deps: Update Rust crate proc-macro2 to v1.0.104 ([8cea2fe](https://github.com/rash-sh/rash/commit/8cea2fec752f25b4d17a397848a79bcbd3cd3f6b))
- deps: Update Rust crate syn to v2.0.112 ([e3ec278](https://github.com/rash-sh/rash/commit/e3ec27868df7559eb6e51bfe853b9dff67437a1d))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v42.69.2 ([dc8ffd8](https://github.com/rash-sh/rash/commit/dc8ffd8c103204e3caf706562b9b9b33c646fb4a))
- deps: Update Rust crate clap to v4.5.54 ([4000bdb](https://github.com/rash-sh/rash/commit/4000bdb0a6ee3567abb2f50e1bd9ccbd4d05a9bc))
- deps: Update Rust crate syn to v2.0.113 ([a80bda0](https://github.com/rash-sh/rash/commit/a80bda0984e813938b2a24a0614f20989f5d15d0))
- deps: Update Rust crate proc-macro2 to v1.0.105 ([aed866e](https://github.com/rash-sh/rash/commit/aed866ee6d393ef0bd0c1f757dc6f6f4240b7a20))
- deps: Update Rust crate quote to v1.0.43 ([fa8c671](https://github.com/rash-sh/rash/commit/fa8c6715c4540622d66929e1874ea7320b9f00fb))
- deps: Update Rust crate serde_json to v1.0.149 ([54d9876](https://github.com/rash-sh/rash/commit/54d9876cef83782278294d8410b59e8295e1ad84))
- deps: Update Rust crate syn to v2.0.114 ([ec83012](https://github.com/rash-sh/rash/commit/ec83012ab28138e1a40bf9edf4334a7900d2c029))
- deps: Update Rust crate chrono to v0.4.43 ([f162779](https://github.com/rash-sh/rash/commit/f162779ae5a458c7e5d1007156e2f145ddc79fa9))
- deps: Update Rust crate prs-lib to v0.5.6 ([c82186c](https://github.com/rash-sh/rash/commit/c82186c1a7d696f941c9b4a16b130b24146ec223))
- deps: Update Rust crate tokio to v1.49.0 ([6b0549a](https://github.com/rash-sh/rash/commit/6b0549a5c5f5528bfb2fd1541b7fb6cc7d2dc0c1))
- deps: Update pre-commit hook adrienverge/yamllint to v1.38.0 ([9a548fb](https://github.com/rash-sh/rash/commit/9a548fb11e2908dc364322e9f6a28530b8f38192))
- deps: Update pre-commit hook alessandrojcm/commitlint-pre-commit-hook to v9.24.0 ([5032aa2](https://github.com/rash-sh/rash/commit/5032aa29abaefa8c740c0fdf8f7d9940741287a3))

## [v2.17.5](https://github.com/rash-sh/rash/tree/v2.17.5) - 2025-12-25

### Fixed

- module: Remove output message when no changes in user and group ([ad2e590](https://github.com/rash-sh/rash/commit/ad2e5907e38362bd85c6bb38281d25661af533a5))

## [v2.17.4](https://github.com/rash-sh/rash/tree/v2.17.4) - 2025-12-24

### Build

- ci: Fix release race condition with dedicated publish ([26f4b99](https://github.com/rash-sh/rash/commit/26f4b99a04b13559541fb860e188a5d8db837e02))

## [v2.17.3](https://github.com/rash-sh/rash/tree/v2.17.3) - 2025-12-24

### Added

- jinja: Add minijinja unicode, urlencode and builtins features ([8e9a58d](https://github.com/rash-sh/rash/commit/8e9a58d38585b86ecbed08245fcf75e02551f180))
- module: Add dconf ([433cf6e](https://github.com/rash-sh/rash/commit/433cf6e34ef2be997cd84ba8bd9e7f5a6c29d3ba))
- task: Add environment variable support ([cff2133](https://github.com/rash-sh/rash/commit/cff21339222792af31aeea20f3000fc5ef351677))

### Fixed

- module: Handle symlinks in copy module ([ca13add](https://github.com/rash-sh/rash/commit/ca13add368c76b024464b5e8d1dc0cbaabc12764))
- module: Skip usermod when appending groups user already has in user module ([f5decda](https://github.com/rash-sh/rash/commit/f5decdad2a63490d1d4bbcf86b2762ff2234b4d8))
- Ensure CHANGELOG commit IDs are correct on release process ([4ee202d](https://github.com/rash-sh/rash/commit/4ee202d35edd24acf74c30d2b4bf9dda5250d303))
- Make clippy happy ([87e6d8b](https://github.com/rash-sh/rash/commit/87e6d8b37028219182c0303da80998e56e157efd))

### Documentation

- Add comprehensive CLI reference documentation ([e389d71](https://github.com/rash-sh/rash/commit/e389d71b1b9f2bf21b883fdedf0707c9f27be2f3))

### Build

- deps: Update Rust crate syn to v2.0.109 ([0071d14](https://github.com/rash-sh/rash/commit/0071d1424219a482490e594cd41f7077e90f2dc5))
- deps: Update Rust crate schemars to v1.1.0 ([70923d6](https://github.com/rash-sh/rash/commit/70923d6634ae7ae75012108755b72464bb0e2c5c))
- deps: Update Rust crate quote to v1.0.42 ([f7b3246](https://github.com/rash-sh/rash/commit/f7b3246aaea712c8147d18763c07d5f3b14c196a))
- deps: Update Rust crate syn to v2.0.110 ([3ad6cd6](https://github.com/rash-sh/rash/commit/3ad6cd602871d6f23a8f7be08389cc01c94d872c))
- deps: Update rust Docker tag to v1.91.1 ([cac9fb2](https://github.com/rash-sh/rash/commit/cac9fb2ea2a654ce35e657267d7f413871f518cb))
- deps: Update Rust crate clap to v4.5.52 ([594b37b](https://github.com/rash-sh/rash/commit/594b37b653fb20b68c1056d09d9a326b0a39983e))
- deps: Update Rust crate serde_with to v3.16.0 ([5948636](https://github.com/rash-sh/rash/commit/594863645fe9705e704e13fa8d10eac8323b108a))
- deps: Upgrade mdbook to 0.5 ([eb89556](https://github.com/rash-sh/rash/commit/eb895563c2f1ca458057f693995de340790fa57a))
- deps: Update Rust crate clap to v4.5.53 ([3661f5e](https://github.com/rash-sh/rash/commit/3661f5ec8e63d3a6faf9df3e247423447e0b6f26))
- deps: Update actions/checkout action to v6 ([95e1d47](https://github.com/rash-sh/rash/commit/95e1d477ebdc9768c20fb4931a79d3e756d0a7e5))
- deps: Update Rust crate syn to v2.0.111 ([9f63991](https://github.com/rash-sh/rash/commit/9f639910a2b1b60534a34b1ac03af836eeed3d72))
- deps: Update Rust crate serde_with to v3.16.1 ([22851f6](https://github.com/rash-sh/rash/commit/22851f6e3dd79f47cf53b97ae36ccbe110660996))
- deps: Update Rust crate minijinja to v2.13.0 ([1853875](https://github.com/rash-sh/rash/commit/185387553376a0df58fbd9f75a24ae0596adcc1b))
- deps: Update Rust crate criterion to 0.8.0 ([7fbe8cd](https://github.com/rash-sh/rash/commit/7fbe8cd9a85c0494a6623d3357a7c18192ac3575))
- deps: Update Rust crate byte-unit to v5.2.0 ([2b0595b](https://github.com/rash-sh/rash/commit/2b0595bfb4c5fda73afb6499cb17f1f753772664))
- deps: Update Rust crate mdbook-driver to v0.5.1 ([6781027](https://github.com/rash-sh/rash/commit/6781027e9ebfa2ba58150d76fa944d354f425c52))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v42 ([a609c07](https://github.com/rash-sh/rash/commit/a609c07eb36913df97665ce2302199ec6fcd42d3))
- deps: Update Rust crate reqwest to v0.12.25 ([1d9d5a5](https://github.com/rash-sh/rash/commit/1d9d5a5eae0c6d92ebcf880618ae8dd55bc65f8d))
- deps: Update Rust crate criterion to v0.8.1 ([2c3abf5](https://github.com/rash-sh/rash/commit/2c3abf5fb4294a1c85092c664f01d0742e646d1a))
- deps: Update Rust crate log to v0.4.29 ([c1fc352](https://github.com/rash-sh/rash/commit/c1fc352185c826ff899cd9a69ead5ee818e2541f))
- deps: Update rust Docker tag to v1.92.0 ([94829a3](https://github.com/rash-sh/rash/commit/94829a3768bb165981dab59a83f9f39d33473f97))
- deps: Update actions/cache action to v5 ([4f41923](https://github.com/rash-sh/rash/commit/4f41923d16e5c8f88ded9739598c652736e07223))
- deps: Update Rust crate mdbook-driver to v0.5.2 ([76c4612](https://github.com/rash-sh/rash/commit/76c46124e82efedaeb7022551b007a9abecbfb65))
- deps: Update Rust crate minijinja to v2.14.0 ([0ffcca2](https://github.com/rash-sh/rash/commit/0ffcca27cae72ef898baf3bcea79a4b38f30d41b))
- deps: Update Rust crate reqwest to v0.12.26 ([4c6e9ca](https://github.com/rash-sh/rash/commit/4c6e9caa07874d31f3f353836c429357d2038c04))
- deps: Update Rust crate console to v0.16.2 ([31e19e7](https://github.com/rash-sh/rash/commit/31e19e715e3eb9377ee9e212da4d701f047249be))
- deps: Update Rust crate serde_json to v1.0.146 ([df37a23](https://github.com/rash-sh/rash/commit/df37a23d2511c978d94cbb9a8c5ea57a099e34ed))
- deps: Update Rust crate serde_json to v1.0.147 ([6a99218](https://github.com/rash-sh/rash/commit/6a992186193d02c63d810815ab7d1ab608677fb5))
- deps: Update Rust crate tempfile to v3.24.0 ([4e161ce](https://github.com/rash-sh/rash/commit/4e161ce5e073a8dc9a65ce2c427a346cae07a48c))
- deps: Update Rust crate reqwest to v0.12.28 ([c625710](https://github.com/rash-sh/rash/commit/c625710b0e6c9a36e3a8d5a4a306f29bb8012b3b))

## [v2.17.2](https://github.com/rash-sh/rash/tree/v2.17.2) - 2025-11-02

### Documentation

- Remove unnecessary changelog header and intro to avoid repetition ([066b822](https://github.com/rash-sh/rash/commit/066b822874912deded2aeaff625564ae39d5e487))
- Fix weights for correct TOC rendering ([5b70763](https://github.com/rash-sh/rash/commit/5b707634a01051a313eca5dfe21d04874c5ea3eb))

## [v2.17.1](https://github.com/rash-sh/rash/tree/v2.17.1) - 2025-11-02

### Fixed

- ci: Re-enable integration tests for MacOS ([398f9bd](https://github.com/rash-sh/rash/commit/398f9bd1259550657afc4ff82c874d202dbefb01))

## [v2.17.0](https://github.com/rash-sh/rash/tree/v2.17.0) - 2025-11-02

### Added

- module: Add user ([73b1cdf](https://github.com/rash-sh/rash/commit/73b1cdf2c661f81e8ce248cf046a58d14c5da133))
- module: Add group ([cd87762](https://github.com/rash-sh/rash/commit/cd8776260a7529f571b295e2648c42c3a441762e))

### Fixed

- task: Remove sum logic for number fields in var merge ([5f0cb7e](https://github.com/rash-sh/rash/commit/5f0cb7e1400ef4e5bdbdeeb98de67ea3614c4805))

### Documentation

- module: Add chars `%^?` to match regex in include_docs ([fed4e2f](https://github.com/rash-sh/rash/commit/fed4e2f6dbd0dea53a0241c0257bbf2e423c5bac))
- Add commit ID links in CHANGELOG.md ([1289df2](https://github.com/rash-sh/rash/commit/1289df296784b4531d019bb109fc0ac7f1548064))

### Build

- ci: Change Apple build to arm64 and update to macos-15 ([e87238a](https://github.com/rash-sh/rash/commit/e87238a08e2f3d027446cfa0c8085ba238048f5d))
  - **BREAKING**: Apple x86_64 binary is deprecated.
- deps: Update Rust crate tokio to v1.48.0 ([9253965](https://github.com/rash-sh/rash/commit/9253965fc1aafcc7151c7b3e97e1f5012fd8de87))
- deps: Update Rust crate reqwest to v0.12.24 ([88a4047](https://github.com/rash-sh/rash/commit/88a40475e53cb6f2a4a1088b76b753036c48b62a))
- deps: Update Rust crate ignore to v0.4.24 ([5b02011](https://github.com/rash-sh/rash/commit/5b020118dc506e224cd0b47cc143c336d41fd0a8))
- deps: Update Rust crate syn to v2.0.107 ([e88e7d1](https://github.com/rash-sh/rash/commit/e88e7d1f912af820338c728cf7dd3857efb99e41))
- deps: Update Rust crate clap to v4.5.50 ([8aae268](https://github.com/rash-sh/rash/commit/8aae2687ddb680b64a46e7097260cab0b4f81212))
- deps: Update Rust crate serde_with to v3.15.1 ([1231b26](https://github.com/rash-sh/rash/commit/1231b2676142e6f308e4aeb538514f244232dc61))
- deps: Update Rust crate syn to v2.0.108 ([00835aa](https://github.com/rash-sh/rash/commit/00835aac5b6c531e4dedd8c5f8bc3f53318c0cbf))
- deps: Update Rust crate proc-macro2 to v1.0.102 ([c330722](https://github.com/rash-sh/rash/commit/c3307225c46d46efd9c44035b87909a023c6ed0e))
- deps: Update Rust crate proc-macro2 to v1.0.103 ([13c8a2e](https://github.com/rash-sh/rash/commit/13c8a2e9660c8dea2da8b720b2dac68c5f8aa151))
- deps: Update Rust crate clap to v4.5.51 ([e646bca](https://github.com/rash-sh/rash/commit/e646bca2f6e030887be2415dc5eca71ffcfc0a91))
- deps: Update Rust crate ignore to v0.4.25 ([29777d9](https://github.com/rash-sh/rash/commit/29777d9ba2a9f04a26dde36a754c408f3d2069a6))
- deps: Update rust Docker tag to v1.91.0 ([7c58d08](https://github.com/rash-sh/rash/commit/7c58d0899a5cf380590a27ac83690e0d9d22da58))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41.168.1 ([27100f9](https://github.com/rash-sh/rash/commit/27100f95a47cffeb74ed02cd8a20dac6b05bbeef))
- deps: Update Rust crate prs-lib to v0.5.5 ([ff4bf4e](https://github.com/rash-sh/rash/commit/ff4bf4e4d52aa2ecb9b693c35bb3fd917ed583a6))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41.168.3 ([39405b6](https://github.com/rash-sh/rash/commit/39405b63c14c40c111f268a876bac404f2128e9f))

## [v2.16.2](https://github.com/rash-sh/rash/tree/v2.16.2) - 2025-10-13

### Fixed

- ci: Add fmt and clippy for build tests
- Make clippy 1.89 happy
- Make clippy 1.90 happy

### Documentation

- Add copilot instructions

### Build

- ci: Fix cargo login token
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.62.1
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41
- deps: Update Rust crate schemars to v1.0.4
- deps: Update Rust crate clap to v4.5.41
- deps: Update Rust crate serde_json to v1.0.141
- deps: Update Rust crate rand to v0.9.2
- deps: Update Rust crate reqwest to v0.12.22
- deps: Update Rust crate serde_with to v3.14.0
- deps: Update Rust crate tokio to v1.46.1
- deps: Update Rust crate mdbook to v0.4.52
- deps: Update strum monorepo to v0.27.2
- deps: Update Rust crate criterion to 0.7.0
- deps: Update Rust crate tokio to v1.47.0
- deps: Update Rust crate ipc-channel to v0.20.1
- deps: Update Rust crate clap to v4.5.42
- deps: Update Rust crate serde_json to v1.0.142
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41.43.0
- deps: Update Rust crate tokio to v1.47.1
- deps: Update Rust crate clap to v4.5.43
- deps: Update Rust crate clap to v4.5.44
- deps: Update Rust crate proc-macro2 to v1.0.97
- deps: Update Rust crate clap to v4.5.45
- deps: Bump slab from 0.4.10 to 0.4.11
- deps: Update Rust crate reqwest to v0.12.23
- deps: Update actions/checkout action to v5
- deps: Update pre-commit hook pre-commit/pre-commit-hooks to v6
- deps: Update rust Docker tag to v1.89.0
- deps: Update Rust crate syn to v2.0.105
- deps: Update Rust crate syn to v2.0.106
- deps: Update Rust crate proc-macro2 to v1.0.98
- deps: Update Rust crate proc-macro2 to v1.0.101
- deps: Update Rust crate serde_json to v1.0.143
- deps: Update Rust crate prs-lib to v0.5.3
- deps: Update Rust crate tempfile to v3.21.0
- deps: Update Rust crate minijinja to v2.12.0
- deps: Update Rust crate regex to v1.11.2
- deps: Update Rust crate clap to v4.5.46
- deps: Update Rust crate clap to v4.5.47
- deps: Update Rust crate log to v0.4.28
- deps: Update actions/setup-python action to v6
- deps: Update clechasseur/rs-clippy-check action to v5
- deps: Update Rust crate tempfile to v3.22.0
- deps: Update Rust crate chrono to v0.4.42
- deps: Update Rust crate console to v0.16.1
- deps: Update Rust crate prs-lib to v0.5.4
- deps: Update Cargo.lock
- deps: Update Rust crate serde to v1.0.220
- deps: Update Rust crate serde_json to v1.0.144
- deps: Update Rust crate serde to v1.0.221
- deps: Update Rust crate semver to v1.0.27
- deps: Update Rust crate serde_json to v1.0.145
- deps: Update Rust crate serde to v1.0.223
- deps: Update Rust crate serde to v1.0.224
- deps: Update Rust crate serde to v1.0.225
- deps: Update Rust crate ipc-channel to v0.20.2
- deps: Update rust Docker tag to v1.90.0
- deps: Update Rust crate clap to v4.5.48
- deps: Update Rust crate serde_with to v3.14.1
- deps: Update Rust crate serde to v1.0.226
- deps: Update Rust crate regex to v1.11.3
- deps: Update Rust crate tempfile to v3.23.0
- deps: Update pre-commit hook alessandrojcm/commitlint-pre-commit-hook to v9.23.0
- deps: Update Rust crate serde to v1.0.228
- deps: Update Rust crate quote to v1.0.41
- deps: Update Rust crate serde_with to v3.15.0
- deps: Update Rust crate regex to v1.12.1
- deps: Update Rust crate clap to v4.5.49
- deps: Update Rust crate regex to v1.12.2

## [v2.16.1](https://github.com/rash-sh/rash/tree/v2.16.1) - 2025-06-30

### Fixed

- module: Improve cmd error and remove Pacman executable detection
- Cargo clippy errors 1.88

### Build

- deps: Update Rust crate schemars to v1.0.3
- deps: Update rust Docker tag to v1.88.0
- deps: Update Rust crate console to 0.16
- deps: Update Rust crate minijinja to v2.11.0

## [v2.16.0](https://github.com/rash-sh/rash/tree/v2.16.0) - 2025-06-25

### Added

- module: Add dereference param to copy

### Fixed

- module: Improve error message on exec not found for pacman module

### Documentation

- ci: Replace master with latest on pages

### Build

- core: Replace serde-yaml with serde-norway
- deps: Update Rust crate syn to v2.0.104
- deps: Update Rust crate schemars to v1
- deps: Fix schemars import on rash_derive
- deps: Update Rust crate schemars to v1.0.1

## [v2.15.0](https://github.com/rash-sh/rash/tree/v2.15.0) - 2025-06-19

### Added

- lookup: Add password
- lookup: Add pipe
- lookup: Add vault
- lookup: Add file

### Fixed

- jinja: Render for invalid string

### Build

- deps: Update Rust crate rand to 0.9

### Refactor

- core: Remove term_size dependency

## [v2.14.2](https://github.com/rash-sh/rash/tree/v2.14.2) - 2025-06-17

### Fixed

- core: Keep `vars` scoped to block execution
- jinja: Improve error message on error

## [v2.14.1](https://github.com/rash-sh/rash/tree/v2.14.1) - 2025-06-16

### Fixed

- module: Propagate variables to parent scope in module block
- task: Render name for `always` and `rescue` tasks

### Build

- ci: Fix package URL on AUR description and use uri module
- ci: Auto update pre-commit once a month automatically
- deps: Update Rust crate sha2 to v0.10.9
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.57.1
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.59.0

### Testing

- ci: Skip Rust hooks on pre-commit workflow
- ci: Deprecate commitlint workflow
- ci: Remove uri and get_url examples
- module: Simplify httpbin.org dependand examples

## [v2.14.0](https://github.com/rash-sh/rash/tree/v2.14.0) - 2025-06-15

### Added

- module: Add uri module
- module: Add get_url module

## [v2.13.0](https://github.com/rash-sh/rash/tree/v2.13.0) - 2025-06-15

### Added

- ci: Add pre-commit and deprecate cargo-husky
- module: Add lineinfile

### Fixed

- module: Show diff on permissions change for copy module

### Documentation

- Update concept map
- Fix changelog of v2.12.0

### Build

- deps: Update Rust crate ipc-channel to v0.20.0
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.56.3

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v2.12.0](https://github.com/rash-sh/rash/tree/v2.12.0) - 2025-06-15

### Added

- task: Add block module
- task: Add `always` and `rescue` attributes

### Fixed

- module: Make clearer messages on diff for systemd module

### Documentation

- task: Add always and rescue attributes with examples

## [v2.11.0](https://github.com/rash-sh/rash/tree/v2.11.0) - 2025-06-15

### Added

- module: Add systemd

## [v2.10.0](https://github.com/rash-sh/rash/tree/v2.10.0) - 2025-06-15

### Added

- module: Add setup module for loading variables from config files

### Build

- deps: Update Rust crate tempfile to v3.20.0
- deps: Update Rust crate mdbook to v0.4.51
- deps: Update Rust crate schemars to 0.9
- deps: Update Rust crate clap to v4.5.40
- deps: Update Rust crate syn to v2.0.102
- deps: Update Rust crate syn to v2.0.103
- deps: Update Rust crate serde_with to v3.13.0

## [v2.9.12](https://github.com/rash-sh/rash/tree/v2.9.12) - 2025-06-08

### Fixed

- task: Include task vars on module exec

### Documentation

- Fix index ref in Rash book

### Build

- deps: Update Rust crate nix to 0.30
- deps: Update Rust crate nix to v0.30.1
- deps: Update Rust crate minijinja to v2.10.1
- deps: Update Rust crate minijinja to v2.10.2
- deps: Update Rust crate clap to v4.5.38
- deps: Update rust Docker tag to v1.87.0
- deps: Update Rust crate criterion to 0.6.0
- deps: Update Rust crate clap to v4.5.39
- Update nix package to 0.30 and Cargo.lock

## [v2.9.11](https://github.com/rash-sh/rash/tree/v2.9.11) - 2025-04-30

### Documentation

- Revert rename index to introduction

### Build

- deps: Update Rust crate chrono to v0.4.41

## [v2.9.10](https://github.com/rash-sh/rash/tree/v2.9.10) - 2025-04-27

### Documentation

- Rename index to introduction

### Build

- ci: Migrate config renovate.json5
- ci: Update ubuntu runners
- deps: Update rust Docker tag to v1.86.0
- deps: Update Rust crate clap to v4.5.36
- deps: Bump crossbeam-channel from 0.5.13 to 0.5.15
- deps: Update Rust crate clap to v4.5.37
- deps: Update Rust crate proc-macro2 to v1.0.95
- deps: Update Rust crate syn to v2.0.101
- Update `ipc-channel` to `82f6c49`

### Testing

- ci: Remove body max line length limitation in commitlint

## [v2.9.9](https://github.com/rash-sh/rash/tree/v2.9.9) - 2025-04-01

### Build

- ci: Fix curl rash installation
- deps: Update Rust crate mdbook to v0.4.48
- deps: Update Rust crate env_logger to v0.11.8

## [v2.9.8](https://github.com/rash-sh/rash/tree/v2.9.8) - 2025-04-01

### Build

- ci: Add sha256sum to release binaries
- ci: Add AUR bin package
- ci: Fix sha256 calc on macOs and add them to gitignore
- deps: Update Rust crate minijinja to v2.9.0
- deps: Update Rust crate clap to v4.5.35

### Refactor

- ci: Format YAML

## [v2.9.7](https://github.com/rash-sh/rash/tree/v2.9.7) - 2025-03-26

### Fixed

- core: Add hashset in expand_usages to avoid re-analyzing candidates
- core: Allow targets and option params with `=` in docopt

### Documentation

- Update README with basic example

### Build

- deps: Update Rust crate log to v0.4.27

## [v2.9.6](https://github.com/rash-sh/rash/tree/v2.9.6) - 2025-03-25

### Fixed

- core: Improve docopt parsing performance pruning option usages
- core: Replace smallest regex by ordering matches in docopt
- core: Support option params with `=` in docopt

### Documentation

- core: Fix options usage and add a test

### Testing

- core: Replace `.py` with `.rh` in docopt

## [v2.9.5](https://github.com/rash-sh/rash/tree/v2.9.5) - 2025-03-23

### Fixed

- core: Change iter to VecDeque to avoid stack overflow calc usages

## [v2.9.4](https://github.com/rash-sh/rash/tree/v2.9.4) - 2025-03-22

### Fixed

- core: Docopt args replace `-` with `_` in vars

## [v2.9.3](https://github.com/rash-sh/rash/tree/v2.9.3) - 2025-03-20

### Fixed

- core: Docopt edge cases with multiple options and commands
- core: Now docopt positional arguments in uppercase are supported

### Documentation

- core: Enhance docopt section with examples and clarifications

### Build

- deps: Update rust Docker tag to v1.85.1
- deps: Update Rust crate tempfile to v3.19.1

## [v2.9.2](https://github.com/rash-sh/rash/tree/v2.9.2) - 2025-03-18

### Fixed

- core: Fixes docopt command that contains dashes

### Build

- core: Update to 2024 edition
- deps: Update Rust crate clap to v4.5.29
- deps: Update Rust crate tempfile to v3.17.1
- deps: Update Rust crate clap to v4.5.30
- deps: Update strum monorepo to v0.27.1
- deps: Update Rust crate mdbook to v0.4.45
- deps: Update KSXGitHub/github-actions-deploy-aur action to v4
- deps: Update Rust crate serde_json to v1.0.139
- deps: Update Rust crate serde to v1.0.218
- deps: Update Rust crate clap to v4.5.31
- deps: Update Rust crate proc-macro2 to v1.0.94
- deps: Update Rust crate serde_json to v1.0.140
- deps: Update Rust crate quote to v1.0.39
- deps: Update Rust crate syn to v2.0.99
- deps: Update Rust crate semver to v1.0.26
- deps: Update Rust crate log to v0.4.26
- deps: Update Rust crate schemars to v0.8.22
- deps: Update Rust crate chrono to v0.4.40
- deps: Update Rust crate console to v0.15.11
- deps: Update Rust crate minijinja to v2.8.0
- deps: Update Rust crate tempfile to v3.18.0
- deps: Update Rust crate serde to v1.0.219
- deps: Update Rust crate syn to v2.0.100
- deps: Update Rust crate clap to v4.5.32
- deps: Update Rust crate quote to v1.0.40
- deps: Update Rust crate mdbook to v0.4.47
- deps: Update Rust crate env_logger to v0.11.7
- deps: Update Rust crate tempfile to v3.19.0
- deps: Update rust Docker tag to v1.85.0

## [v2.9.1](https://github.com/rash-sh/rash/tree/v2.9.1) - 2025-02-09

### Fixed

- ci: Clean `release.sh` duplicated steps
- Cargo clippy errors

### Documentation

- lookup: Update find example with new minijinja sintax

### Build

- ci: Use mdbook version from `Cargo.lock`
- ci: Disables concurrent builds in pages deploy
- deps: Update Rust crate clap to v4.5.21
- deps: Update Rust crate serde_json to v1.0.133
- deps: Update Rust crate byte-unit to v5.1.6
- deps: Update Rust crate mdbook to v0.4.42
- deps: Update Rust crate serde to v1.0.215
- deps: Update Rust crate syn to v2.0.87
- deps: Update Rust crate tempfile to v3.14.0
- deps: Update Rust crate prs-lib to v0.5.2
- deps: Update Rust crate syn to v2.0.89
- deps: Update Rust crate proc-macro2 to v1.0.92
- deps: Update Rust crate mdbook to v0.4.43
- deps: Update Rust crate syn to v2.0.90
- deps: Update rust Docker tag to v1.83.0
- deps: Update Rust crate clap to v4.5.22
- deps: Update Rust crate clap to v4.5.23
- deps: Update Rust crate serde to v1.0.216
- deps: Update Rust crate chrono to v0.4.39
- deps: Update Rust crate semver to v1.0.24
- deps: Update Rust crate fern to v0.7.1
- deps: Update Rust crate console to v0.15.10
- deps: Update wagoid/commitlint-github-action action to v6.2.0
- deps: Update Rust crate serde_json to v1.0.134
- deps: Update Rust crate syn to v2.0.91
- deps: Update Rust crate quote to v1.0.38
- deps: Update Rust crate syn to v2.0.92
- deps: Update Rust crate serde to v1.0.217
- deps: Update Rust crate syn to v2.0.93
- deps: Update Rust crate syn to v2.0.94
- deps: Update Rust crate syn to v2.0.95
- deps: Update Rust crate serde_json to v1.0.135
- deps: Update Rust crate clap to v4.5.24
- deps: Update Rust crate minijinja to v2.6.0
- deps: Update Rust crate clap to v4.5.26
- deps: Update Rust crate syn to v2.0.96
- deps: Update Rust crate proc-macro2 to v1.0.93
- deps: Update Rust crate env_logger to v0.11.6
- deps: Update Rust crate serde_with to v3.12.0
- deps: Update Rust crate itertools to 0.14
- deps: Update Rust crate tempfile to v3.15.0
- deps: Update rust Docker tag to v1.84.0
- deps: Update clechasseur/rs-clippy-check action to v4
- deps: Update wagoid/commitlint-github-action action to v6.2.1
- deps: Update Rust crate serde_json to v1.0.136
- deps: Update Rust crate semver to v1.0.25
- deps: Update Rust crate serde_json to v1.0.137
- deps: Update Rust crate clap to v4.5.27
- deps: Update Rust crate log to v0.4.25
- deps: Update Rust crate similar to v2.7.0
- deps: Update Rust crate serde_json to v1.0.138
- deps: Update rust Docker tag to v1.84.1
- deps: Update Rust crate syn to v2.0.97
- deps: Update Rust crate syn to v2.0.98
- deps: Update Rust crate clap to v4.5.28
- Compile just rash bin in make target build and release
- Filter rash binary in AUR packages

### Refactor

- derive: Simplify returns with `.into()` method
- derive: Remove dead code imports
- derive: Reuse imports

## [v2.9.0](https://github.com/rash-sh/rash/tree/v2.9.0) - 2024-11-11

### Build

- ci: Change images to GitHub registry
- deps: Update Rust crate minijinja to v2.5.0
  - Added a `lines` filter to split a string into lines.
  - Added the missing `string` filter from Jinja2. mitsuhiko/minijinja#617
  - and more: [2.5.0](https://github.com/mitsuhiko/minijinja/releases/tag/2.5.0) and
    [2.4.0](https://github.com/mitsuhiko/minijinja/releases/tag/2.5.0)

### Testing

- cli: Disable e2e tests for ARM

## [v2.8.0](https://github.com/rash-sh/rash/tree/v2.8.0) - 2024-11-07

### Added

- cli: Add `script` argument for inline script
- deps: Enable `loop_controls` feature in minijinja

### Build

- deps: Update Rust crate serde-error to v0.1.3
- deps: Update Rust crate serde to v1.0.214

## [v2.7.6](https://github.com/rash-sh/rash/tree/v2.7.6) - 2024-10-24

### Fixed

- book: Change static to const
- ci: Clippy Github Action name typo
- task: Delete `special.rs` file not in use
- Formatting issues

### Build

- deps: Update Rust crate proc-macro2 to v1.0.87
- deps: Update Rust crate clap to v4.5.20
- deps: Update Rust crate proc-macro2 to v1.0.88
- deps: Update Rust crate ipc-channel to 0.19
- deps: Update Rust crate serde_json to v1.0.129
- deps: Update Rust crate serde_json to v1.0.130
- deps: Update Rust crate serde_json to v1.0.131
- deps: Update Rust crate serde_json to v1.0.132
- deps: Update Rust crate syn to v2.0.80
- deps: Update Rust crate syn to v2.0.81
- deps: Update Rust crate syn to v2.0.82
- deps: Update rust Docker tag to v1.82.0
- deps: Update Rust crate serde to v1.0.211
- deps: Update Rust crate proc-macro2 to v1.0.89
- deps: Update Rust crate serde to v1.0.212
- deps: Update Rust crate serde to v1.0.213
- deps: Update Rust crate syn to v2.0.83
- deps: Update Rust crate syn to v2.0.84
- deps: Update Rust crate syn to v2.0.85
- deps: Update Rust crate regex to v1.11.1
- deps: Update Rust crate fern to 0.7.0

### Refactor

- core: Remove String from function arg
- Refactored get_module_name method

## [v2.7.5](https://github.com/rash-sh/rash/tree/v2.7.5) - 2024-10-06

### Build

- Add jemalloc for musl

## [v2.7.4](https://github.com/rash-sh/rash/tree/v2.7.4) - 2024-10-06

### Documentation

- vars: Fix debug function call

### Build

- deps: Update Rust crate clap to v4.5.18
- deps: Update Rust crate syn to v2.0.79
- deps: Update Rust crate tempfile to v3.13.0
- deps: Update Rust crate regex to v1.11.0
- deps: Update Rust crate clap to v4.5.19
- deps: Update Rust crate serde_with to v3.10.0
- deps: Update Rust crate serde_with to v3.11.0
- deps: Update Rust crate ipc-channel to v0.18.3
- Optimize release binary

## [v2.7.3](https://github.com/rash-sh/rash/tree/v2.7.3) - 2024-09-18

### Added

- ci: Add release.sh script

### Fixed

- vars: Make `rash.path` canonical for coherence with `rash.dir`

### Build

- deps: Update Rust crate **minijinja** to v2.3.1
- deps: Update Rust crate clap to v4.5.17
- deps: Update Rust crate serde_json to v1.0.128
- deps: Update Rust crate serde to v1.0.210
- deps: Update rust to v1.81
- deps: Update Rust crate syn to v2.0.77
- deps: Update Rust crate ignore to v0.4.23
- deps: Remove pinned versions from `Cargo.toml`
- docker: Update target base image version to trixie-20240904-slim
- Remove death code

### Testing

- module: Add e2e for include

## [v2.7.2](https://github.com/rash-sh/rash/tree/v2.7.2) - 2024-09-16

### Fixed

- task: Add serde to handle result from fork in become tasks

### Documentation

- lookup: Add example and comments to passwordstore examples
- Add to changelog missing info for v2.7.1

### Refactor

- vars: Simplify the builtin vars implementation

## [v2.7.1](https://github.com/rash-sh/rash/tree/v2.7.1) - 2024-09-15

### Fixed

- core: Add script path to task name output
- module: Include continue workflow in the previous context

## [v2.7.0](https://github.com/rash-sh/rash/tree/v2.7.0) - 2024-09-15

### Added

- lookup: Add `subkey` option to passwordstore

### Build

- deps: Change clippy to clechasseur/rs-clippy-check action to v3

## [v2.6.0](https://github.com/rash-sh/rash/tree/v2.6.0) - 2024-09-15

### Added

- module: Add include

### Documentation

- Update dotfiles example refactorized

## [v2.5.0](https://github.com/rash-sh/rash/tree/v2.5.0) - 2024-09-10

### Added

- lookup: Add `returnall` option to passwordstore

## [v2.4.0](https://github.com/rash-sh/rash/tree/v2.4.0) - 2024-09-10

### Added

- module: Make `render_params` force string optional

### Fixed

- ci: Remove `fetch-depth: 0` to get just last commit on commitlint
- ci: Add permissions to commitlint action

### Documentation

- lookup: Remove TODO as completed
- Add find lookup example and update dots script
- Update dots example

### Build

- deps: Update Rust crate syn to v2.0.75
- deps: Update wagoid/commitlint-github-action action to v6.1.0
- deps: Update wagoid/commitlint-github-action action to v6.1.1
- deps: Update KSXGitHub/github-actions-deploy-aur action to v3
- deps: Update Rust crate quote to v1.0.37
- deps: Update Rust crate serde_json to v1.0.127
- deps: Update Rust crate serde to v1.0.209
- deps: Update Rust crate syn to v2.0.76
- deps: Update Rust crate minijinja to v2.2.0
- deps: Update KSXGitHub/github-actions-deploy-aur action to v3.0.1
- deps: Update wagoid/commitlint-github-action action to v6.1.2
- deps: Update rust Docker tag to v1.81.0

### Refactor

- core: Merge `minijinja::Value` instead of using json
- core: Replace minijinja value by serde_json in docopt
- core: Improbe `merge_json` performance
- core: Small tweak in parse function in docopt
- jinja: Expose render with `force_string` functions
- jinja: Improve `Value` transformations
- lookup: Direct serde between `Params` and `minijinja::Value`

### Testing

- module: Add `set_vars.rh` to examples

## [v2.3.1](https://github.com/rash-sh/rash/tree/v2.3.1) - 2024-08-15

### Fixed

- task: Render iterator when item used in vars

### Documentation

- Order changelog groups

## [v2.3.0](https://github.com/rash-sh/rash/tree/v2.3.0) - 2024-08-15

### Added

- lookup: Add find reusing module logic

### Build

- deps: Update Rust crate serde_json to v1.0.125
- deps: Update Rust crate serde to v1.0.208

### Fixed

- task: Support `omit` in `vars`
- task: Render params recursivey and respect omit
- task: Use vars to render iterator loop

## [v2.2.0](https://github.com/rash-sh/rash/tree/v2.2.0) - 2024-08-14

### Build

- deps: Update Rust crate serde to v1.0.207

### Fixed

- jinja: Omit not trigger error when default variable exists
  - **BREAKING**: use `default(omit)` instead of `default(omit())`.

## [v2.1.1](https://github.com/rash-sh/rash/tree/v2.1.1) - 2024-08-11

### Build

- deps: Update Rust crate serde_json to v1.0.123

### Fixed

- task: Render vars recursively

## [v2.1.0](https://github.com/rash-sh/rash/tree/v2.1.0) - 2024-08-11

### Added

- jinja: Enable `tojson` filter from minijinja
- lookup: Add passwordstore

### Build

- deps: Update Rust crate clap to v4.5.15
- deps: Update Rust crate syn to v2.0.73
- deps: Update Rust crate serde to v1.0.206
- deps: Update Rust crate syn to v2.0.74

### Documentation

- jinja: Add lookups programmatically to Rash book
- jinja: Add section with lookups and filters
- Replace Tera doc with MiniJinja
- Add debug vars and context info
- Fix index

### Fixed

- module: `set_vars` overwrites previous variables

### Refactor

- jinja: Add macro for generating add lookup function
- module: Move module::utils to utils
- task: Change `test_render_params_with_vars_array_concat`
- Create jinja module

### Testing

- task: Add vars concat arrays test

## [v2.0.1](https://github.com/rash-sh/rash/tree/v2.0.1) - 2024-08-09

### Build

- Remove armhf build

### Documentation

- Update examples with MiniJinja breacking changes

### Fixed

- Minor docs and refactors

### Refactor

- Use minijinja::Value instead of Vars abstraction

### Testing

- task: Check item is removed from vars after execute loop task

## [v2.0.0](https://github.com/rash-sh/rash/tree/v2.0.0) - 2024-08-09

### **BREAKING**

Replaced Tera with Minijinja, enhancing the project's versatility and bringing near-complete
compatibility with Jinja2 syntax. This upgrade resolves several critical issues, including improved
handling of `()` in expressions.

With Minijinja, Rash now overcomes the limitations previously imposed by the Jinja2 engine.

### Build

- deps: Update Rust crate serde to v1.0.204
- deps: Update Rust crate syn to v2.0.69
- deps: Update Rust crate syn to v2.0.70
- deps: Update Rust crate clap to v4.5.9
- deps: Update Rust crate syn to v2.0.71
- deps: Update Rust crate syn to v2.0.72
- deps: Update Rust crate clap to v4.5.10
- deps: Update Rust crate similar to v2.6.0
- deps: Update Rust crate serde_with to v3.9.0
- deps: Update Rust crate env_logger to v0.11.4
- deps: Update Rust crate clap to v4.5.11
- deps: Update Rust crate serde_json to v1.0.121
- deps: Update Rust crate clap to v4.5.12
- deps: Update Rust crate clap to v4.5.13
- deps: Update Rust crate serde_json to v1.0.122
- deps: Update Rust crate regex to v1.10.6
- deps: Update wagoid/commitlint-github-action action to v6.0.2
- deps: Update Rust crate tempfile to v3.12.0
- deps: Update Rust crate serde to v1.0.205
- deps: Update Rust crate clap to v4.5.14
- deps: Update rust Docker tag to v1.80.1

### Documentation

- Change from list to script in release workflow

### Refactor

- tera: Change Jinja2 engine for minijinja
- Replace lazy_static with std from 1.80

## [v1.10.5](https://github.com/rash-sh/rash/tree/v1.10.5) - 2024-07-04

### Fixed

- module: Not display for Content::Bytes in Copy

### Refactor

- module: Improve readalability in Copy

## [v1.10.4](https://github.com/rash-sh/rash/tree/v1.10.4) - 2024-07-04

### Build

- deps: Update Rust crate serde_json to v1.0.118
- deps: Update Rust crate log to v0.4.22
- deps: Update Rust crate clap to v4.5.8
- deps: Update Rust crate serde_json to v1.0.119
- deps: Update Rust crate serde_with to v3.8.2
- deps: Update Rust crate serde_json to v1.0.120
- deps: Update KSXGitHub/github-actions-deploy-aur action to v2.7.2
- deps: Update Rust crate serde_with to v3.8.3

### Fixed

- module: Copy binary data

## [v1.10.3](https://github.com/rash-sh/rash/tree/v1.10.3) - 2024-06-24

### Build

- deps: Update Rust crate lazy_static to v1.5.0
- deps: Update Rust crate syn to v2.0.68
- deps: Update Rust crate strum to v0.26.3
- Fix AUR gpg key fingerprint

## [v1.10.2](https://github.com/rash-sh/rash/tree/v1.10.2) - 2024-06-21

### Added

- ci: Add automerge in patch versions for renovate
- ci: Add autotag workflow

### Build

- deps: Update Rust crate nix to 0.28
- deps: Update softprops/action-gh-release action to v2
- deps: Update mindsers/changelog-reader-action action to v2.2.3
- deps: Bump mio from 0.8.10 to 0.8.11
- deps: Update Rust crate serde_with to 3.7
- deps: Update wagoid/commitlint-github-action action to v6
- deps: Update Rust crate similar to 2.5
- deps: Update rust Docker tag to v1.77.0
- deps: Update rust Docker tag to v1.77.1
- deps: Update KSXGitHub/github-actions-deploy-aur action to v2.7.1
- deps: Update wagoid/commitlint-github-action action to v6.0.1
- deps: Update Rust crate serde_with to 3.8
- deps: Update Rust crate clap to 4.5.4
- deps: Update Rust crate criterion to 0.5.1
- deps: Update rust Docker tag to v1.77.2
- deps: Update Rust crate byte-unit to 5.1.4
- deps: Update Rust crate cargo-husky to 1.5.0
- deps: Update Rust crate chrono to 0.4.38
- deps: Update Rust crate fern to 0.6.2
- deps: Update Rust crate env_logger to 0.11.3
- deps: Update Rust crate ignore to 0.4.22
- deps: Update Rust crate itertools to 0.12.1
- deps: Update Rust crate log to 0.4.21
- deps: Update Rust crate proc-macro2 to 1.0.81
- deps: Update Rust crate regex to 1.10.4
- deps: Update Rust crate semver to 1.0.22
- deps: Update Rust crate serde to 1.0.200
- deps: Update Rust crate serde_with to 3.8.1
- deps: Update Rust crate serde_json to 1.0.116
- deps: Update Rust crate strum to 0.26.2
- deps: Update Rust crate quote to 1.0.36
- deps: Update Rust crate schemars to 0.8.17
- deps: Update Rust crate serde-error to 0.1.2
- deps: Update Rust crate tempfile to 3.10.1
- deps: Update rust Docker tag to v1.78.0
- deps: Update Rust crate strum_macros to 0.26.2
- deps: Update Rust crate tera to 1.19.1
- deps: Update Rust crate syn to 2.0.60
- deps: Update Rust crate serde_yaml to v0.9.34
- deps: Update Rust crate schemars to v0.8.18
- deps: Update Rust crate semver to v1.0.23
- deps: Update Rust crate proc-macro2 to v1.0.82
- deps: Update Rust crate serde to v1.0.201
- deps: Update Rust crate serde_json to v1.0.117
- deps: Update Rust crate syn to v2.0.62
- deps: Update Rust crate syn to v2.0.63
- deps: Update Rust crate serde to v1.0.202
- deps: Update Rust crate schemars to v0.8.19
- deps: Update peaceiris/actions-gh-pages action to v4
- deps: Update Rust crate mdbook to v0.4.38
- deps: Update Rust crate syn to v2.0.64
- deps: Update Rust crate itertools to 0.13.0
- deps: Update Rust crate mdbook to v0.4.40
- deps: Update Rust crate proc-macro2 to v1.0.83
- deps: Update Rust crate syn to v2.0.65
- deps: Update Rust crate schemars to v0.8.20
- deps: Update Rust crate schemars to v0.8.21
- deps: Update Rust crate syn to v2.0.66
- deps: Update Rust crate nix to 0.29
- deps: Update Rust crate proc-macro2 to v1.0.84
- deps: Update Rust crate serde to v1.0.203
- deps: Update Rust crate ipc-channel to v0.18.1
- deps: Update Rust crate proc-macro2 to v1.0.85
- deps: Update Rust crate strum_macros to v0.26.3
- deps: Update Rust crate clap to v4.5.6
- deps: Update Rust crate strum_macros to v0.26.4
- deps: Update Rust crate regex to v1.10.5
- deps: Update Rust crate clap to v4.5.7
- deps: Update Rust crate syn to v2.0.67
- deps: Update Rust crate proc-macro2 to v1.0.86
- deps: Update rust Docker tag to v1.79.0
- deps: Update Rust crate tera to v1.20.0

### Fixed

- ci: Automerge all patches
- Cargo clippy warnings

## [v1.10.1](https://github.com/rash-sh/rash/tree/v1.10.1) - 2024-02-23

### Added

- ci: Add renovate
- module: Add pacman
- module: Check pacman upgrades before execution

### Build

- book: Update mdbook to 0.4.34
- deps: Bump rustix from 0.37.23 to 0.37.25
- deps: Bump unsafe-libyaml from 0.2.9 to 0.2.10
- deps: Bump shlex from 1.2.0 to 1.3.0
- deps: Update Rust crate mdbook to 0.4.37
- deps: Update KSXGitHub/github-actions-deploy-aur action to v2.7.0
- deps: Update Rust crate term_size to 1.0.0-beta1
- deps: Update Rust crate itertools to 0.12
- deps: Update Rust crate regex to 1.10
- deps: Update Rust crate serde_with to 3.6
- deps: Update Rust crate strum to 0.26
- deps: Update Rust crate console to 0.15.8
- deps: Update Rust crate term_size to 1.0.0-beta.2
- deps: Update wagoid/commitlint-github-action action to v5
- deps: Update docker/setup-qemu-action action to v3
- deps: Update docker/setup-buildx-action action to v3
- deps: Update actions/checkout action to v4
- deps: Update rust Docker tag to v1.76.0
- deps: Update mindsers/changelog-reader-action action to v2.2.2
- deps: Update Rust crate env_logger to 0.11
- deps: Update Rust crate ipc-channel to 0.18
- deps: Update Rust crate similar to 2.4
- deps: Update Rust crate strum_macros to 0.26
- deps: Update Rust crate clap to 4.5
- deps: Update Rust crate byte-unit to v5
- deps: Update lock file
- docker: Update debian to latest bookworm version
- Compress binary with upx
- Fix macOS and push images
- Increase min rust version to 1.74

### Documentation

- ci: Remove patch versions from web page
- core: Add comment about tera bug
- module: Include pacman examples and remove new lines in params
- vars: Add debug command to show all vars in current context

### Fixed

- ci: Fix strip ref prefix from version in github pages action
- core: Log errors instead of trace
- core: Enable vars in when param
- core: Add log trace for extend vars
- core: Allow module log for empty output
- core: Log with colors just if terminal
- docker: Update to rust 1.72.0
- docker: Update to rust 1.75.0

### Refactor

- core: Replace match with and_then for readibility
- module: Add run_test function for pacman integration tests
- Replace to_string to to_owner when possible
- Remove match in favor of map if possible
- Remove some match statements

### Testing

- Add docopt benches

## [v1.10.0](https://github.com/rash-sh/rash/tree/v1.10.0) - 2023-09-12

### Added

- core: Add output option to print log raw mode

### Fixed

- ci: Run jobs just in PR or master branch
- deps: Remove users crate dependency

## [v1.9.0](https://github.com/rash-sh/rash/tree/v1.9.0) - 2023-09-07

### Added

- task: Add `vars` optional field

### Build

- Upgrade to Rust 1.70 and fix new clippy warnings
- Update compatible versions
- Upgrade incompatible versions
- Add memfd feature to ipc-channel
- Disable memfd for ipc-channel
- Set resolver = "2"

### Documentation

- Add dotfile description
- Fix readme typo

### Fixed

- ci: Update workers to latest versions
- ci: Upgrade cache action version to v2
- ci: Update to node16 github actions
- ci: Replace `actions-rs/toolchain` with `dtolnay/rust-toolchain`
- ci: Change dtolnay/rust-toolchaint to stable
- ci: Remove container and downgrade to ubuntu 20
- core: Improve docopt performance prefiltering possible options
- core: Handle docopt edge cases with optiona arguments
- task: Improve error message when become fails
- Cargo clippy errors

### Removed

- Command module: `transfer_pid_1` (use `transfer_pid` instead)

## [v1.8.6](https://github.com/rash-sh/rash/tree/v1.8.6) - 2023-01-27

### Added

- module: Support `chdir` in command module

### Build

- book: Update mdbook to 0.4.25
- deps: Bump prettytable-rs from 0.8.0 to 0.10.0
- Upgrade to Rust 1.67 and fix new clippy warnings

### Fixed

- ci: Remove build scope from commitlintrc
- core: Set up to current dir parent path when empty
- module: Add trace for command exec

## [v1.8.5](https://github.com/rash-sh/rash/tree/v1.8.5) - 2022-12-20

### Added

- Add `git-cliff` to update CHANGELOG automatically

### Build

- Upgrade to Rust 1.66 and fix new clippy warnings
- Add arm64 docker images

### Documentation

- Fix build status badget

### Fixed

- ci: Add local versions in dependencies
- cli: Change skipping log to debug

### Refactor

- module: Implement trait Module

## [v1.8.4](https://github.com/rash-sh/rash/tree/v1.8.4) (2022-10-24)

### Fixed

- ci: Read version from `Cargo.toml`

## [v1.8.3](https://github.com/rash-sh/rash/tree/v1.8.3) (2022-10-24) [YANKED]

### Fixed

- cli: Support repeated arguments in docopt (#281)
- cli: Help not ignored when positional required in docopt (#283)
- cli: Improve tera error handling and add a trace all verbose option (#287)
- docs: Add default values and fix examples (#285)

## [v1.8.2](https://github.com/rash-sh/rash/tree/v1.8.2) (2022-08-15)

### Fixed

- Fix multi-word variable repr for options when true in docopt (#274)

## [v1.8.1](https://github.com/rash-sh/rash/tree/v1.8.1) (2022-08-15)

### Fixed

- Fix multi-word variable repr for options in docopt (#273)

## [v1.8.0](https://github.com/rash-sh/rash/tree/v1.8.0) (2022-06-30)

### Added

- Support all data structures in loops (#263)

## [v1.7.1](https://github.com/rash-sh/rash/tree/v1.7.1) (2022-06-13)

### Fixed

- Update Debian image to bullseye and Rust to 1.61.0
- Bumps [regex](https://github.com/rust-lang/regex) from 1.5.4 to 1.5.5.
  - [Release notes](https://github.com/rust-lang/regex/releases)
  - [Changelog](https://github.com/rust-lang/regex/blob/master/CHANGELOG.md)
  - [Commits](https://github.com/rust-lang/regex/compare/1.5.4...1.5.5)
- Update ipc-channel to 0.16 and run `cargo update`

## [v1.7.0](https://github.com/rash-sh/rash/tree/v1.7.0) (2022-01-26)

### Added

- Rename `transfer_pid_1` to `transfer_pid` in command module
- Add module debug (#241)

## [v1.6.1](https://github.com/rash-sh/rash/tree/v1.6.1) (2022-01-22)

### Fixed

- Options variables are now accessible (#236)
- Update to Rust 1.58.1

## [v1.6.0](https://github.com/rash-sh/rash/tree/v1.6.0) (2022-01-20)

### Added

- Add parse options to docopt implementation (#232)
- Use `cross` for musl docker image (#232)

## [v1.5.0](https://github.com/rash-sh/rash/tree/v1.5.0) (2022-01-09)

### Added

- Add become (#220)
- Add `omit()` for omitting parameters programmatically (#70)
- Add preserve mode to copy module (#214)
- Add docopt to `rash` files (#212)

### Fixed

- Format mode in diff as octal in File module

## [v1.4.1](https://github.com/rash-sh/rash/tree/v1.4.1) (2021-12-24)

### Fixed

- Fix log with print in normal diff

## [v1.4.0](https://github.com/rash-sh/rash/tree/v1.4.0) (2021-12-22)

### Added

- Add find module

### Fixed

- Fix `rash.dir` as absolute according with docs
- Fix publish packages to crates.io

## [v1.3.1](https://github.com/rash-sh/rash/tree/v1.3.1) (2021-12-19)

### Added

- Automatically added body to GitHub release

### Fixed

- Update rash package versions in Cargo.lock (missing in 1.3.0)

## [v1.3.0](https://github.com/rash-sh/rash/tree/v1.3.0) (2021-12-19)

### Added

- Add `changed_when` optional field in task
- Add support for arrays in `when` and `changed_when`
- Add clearer logger for diff files
- Add src option to copy module
- Add `check` mode

### Fixed

- Parsed `when` and `changed_when` when they are booleans
- Builtin dir when current dir returns `.`
- Check `when` for each different item in loop
- Remove vendor on release

## [v1.2.0](https://github.com/rash-sh/rash/tree/v1.2.0) (2021-12-17)

### Added

- Add diff param and apply in file, copy and template modules (#190)
- Get params doc from struct (#189)

### Fixed

- Add warn and error to stderr instead of stdout
- Remove `--all-features` from release

## [v1.1.0](https://github.com/rash-sh/rash/tree/v1.1.0) (2021-12-12)

### Added

- Add file module (#180)

## [v1.0.2](https://github.com/rash-sh/rash/tree/v1.0.2) (2021-12-07)

### Added

- Add AUR packages automatic build and publish
- Release with signed tags
- Add releases binaries with Linux Glib >= 2.17 support and macOS

## [v1.0.1](https://github.com/rash-sh/rash/tree/v1.0.1) (2021-12-03)

### Bug fixes

- Remove duplicate error messages

## [v1.0.0](https://github.com/rash-sh/rash/tree/v1.0.0) (2020-06-11)

First stable version released:

### modules

- assert
- command
- copy
- template
- set_vars

### tasks

- when
- register
- ignore_errors
- loop

### vars

- rash
  - args
  - dir
  - path
  - user.uid
  - user.gid
- env

## v0.1.0

Core version released:

- data structure
- error management
- log
- execution
- cli

### modules

- add command (basic functionality)
