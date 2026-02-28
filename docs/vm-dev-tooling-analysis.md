# VM Dev Tooling Modules Analysis

**Issue:** #1340  
**Author:** Analysis for @forkline

## Executive Summary

This document analyzes rash modules critical for installing dev tooling in VMs during boot, identifies gaps, and proposes a roadmap.

---

## 1. Key Modules for VM Dev Tooling

### 1.1 Package Managers (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `apt` | ✅ Complete | Unit | Debian/Ubuntu - Full featured |
| `dnf` | ✅ Complete | Unit + Integration | RHEL/Fedora - Full featured |
| `pacman` | ✅ Complete | Unit + Integration | Arch Linux |
| `apk` | ✅ Complete | Unit + Integration | Alpine Linux |
| `zypper` | ✅ Complete | Unit + Integration | openSUSE |
| `package` | ✅ Complete | Unit | Generic package manager abstraction |

### 1.2 Language Package Managers (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `pip` | ✅ Complete | Unit | Python packages |
| `npm` | ✅ Complete | Unit + Integration | Node.js packages |
| `gem` | ✅ Complete | Unit + Integration | Ruby packages |
| `cargo` | ✅ Complete | Unit + Integration | Rust packages |
| `composer` | ✅ Complete | Unit | PHP packages |

### 1.3 User & Access Management (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `user` | ✅ Complete | Unit + Integration | User management |
| `group` | ✅ Complete | Unit + Integration | Group management |
| `authorized_key` | ✅ Complete | Unit + Integration | SSH key management |
| `known_hosts` | ✅ Complete | Unit | SSH known hosts |

### 1.4 File Operations (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `file` | ✅ Complete | Unit | File/directory management |
| `copy` | ✅ Complete | Unit | Copy files |
| `template` | ✅ Complete | Unit | Jinja2 templates |
| `lineinfile` | ✅ Complete | Unit | Line in file management |
| `ini_file` | ✅ Complete | Unit | INI file management |
| `json_file` | ✅ Complete | Unit | JSON file management |
| `slurp` | ✅ Complete | Unit | Read file contents |

### 1.5 Service Management (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `systemd` | ✅ Complete | Unit + Integration | Systemd services |
| `service` | ✅ Complete | Unit | Generic service management |
| `reboot` | ✅ Complete | Unit + Integration | System reboot |

### 1.6 Network Configuration (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `hostname` | ✅ Complete | Unit + Integration | Hostname management |
| `netplan` | ✅ Complete | Unit + Integration | Ubuntu network config |
| `nmcli` | ✅ Complete | Unit | NetworkManager CLI |
| `interfaces_file` | ✅ Complete | Unit | Debian interfaces |

### 1.7 System Configuration (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `locale` | ✅ Complete | Unit | Locale settings |
| `timezone` | ✅ Complete | Unit + Integration | Timezone configuration |
| `sysctl` | ✅ Complete | Unit | Kernel parameters |
| `pam_limits` | ✅ Complete | Unit + Integration | PAM limits |
| `grub` | ✅ Complete | Unit + Integration | GRUB configuration |

### 1.8 Development Tools (Critical)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `git` | ✅ Complete | Unit | Git repository management |
| `docker_container` | ✅ Complete | Unit | Docker containers |
| `docker_image` | ✅ Complete | Unit | Docker images |
| `unarchive` | ✅ Complete | Unit | Archive extraction |

### 1.9 Repository Management (Important)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `apt_repository` | ✅ Complete | Unit | APT repositories |
| `yum_repository` | ✅ Complete | Unit | YUM/DNF repositories |
| `gpg_key` | ✅ Complete | Unit | GPG key management |

### 1.10 Wait/Async Operations (Important)

| Module | Status | Tests | Notes |
|--------|--------|-------|-------|
| `wait_for` | ✅ Complete | Unit | Wait for conditions |
| `async_status` | ✅ Complete | Unit | Async task management |
| `pause` | ✅ Complete | Unit | Pause execution |

---

## 2. Missing Modules for VM Dev Tooling

### 2.1 High Priority - Should Add

| Module | Purpose | Ansible Equivalent |
|--------|---------|-------------------|
| `flatpak` | Flatpak package management | `community.general.flatpak` |
| `homebrew` | macOS package management | `community.general.homebrew` |
| `chocolatey` | Windows package management | `chocolatey.chocolatey.win_chocolatey` |

### 2.2 Medium Priority - Nice to Have

| Module | Purpose | Ansible Equivalent |
|--------|---------|-------------------|
| `sdkman` | SDK version manager | Custom |
| `nvm` | Node version manager | Custom |
| `pyenv` | Python version manager | Custom |
| `rbenv` | Ruby version manager | Custom |
| `go` | Go installation | Custom |

### 2.3 Low Priority - Specialized

| Module | Purpose | Ansible Equivalent |
|--------|---------|-------------------|
| `vagrant` | Vagrant box management | Custom |
| `packer` | Packer image building | Custom |
| `terraform` | Terraform state | Custom |

---

## 3. Test Coverage Analysis

### 3.1 Modules Without Tests

| Module | Impact |
|--------|--------|
| `set_vars` | Low - Simple module |

### 3.2 Modules with Only Unit Tests (Need Integration Tests)

These modules have unit tests but would benefit from integration tests:

- `apt` - Critical for VM setup
- `pip` - Critical for Python tooling
- `git` - Critical for dev workflows
- `docker_container` - Critical for containerized dev
- `docker_image` - Critical for containerized dev
- `service` - Critical for service management
- `file` - Critical for file operations
- `copy` - Critical for file operations
- `template` - Critical for configuration

---

## 4. Roadmap

### Phase 1: Validation (Current)
- [x] Identify key modules for VM dev tooling
- [x] Analyze current module test coverage
- [ ] Create integration tests for critical modules
- [ ] Document module usage examples

### Phase 2: Gap Filling
- [ ] Implement `flatpak` module (High priority)
- [ ] Add integration tests for apt, pip, git modules

### Phase 3: Enhancement
- [ ] Add version manager modules (sdkman, nvm, pyenv)
- [ ] Improve error messages in existing modules
- [ ] Add idempotency validation tests

---

## 5. Recommended Issues (Created)

### Issue #1343: Add Integration Tests for Critical Package Managers
**Priority:** High  
**Modules:** apt, pip

These modules only have unit tests but are critical for VM setup. Integration tests would validate:
- Package installation/removal
- Cache updates
- Version pinning

### Issue #1344: Implement Flatpak Module  
**Priority:** Medium  
**Description:** Add `flatpak` module for Flatpak package management.

### Issue #1345: Add Git Module Integration Tests
**Priority:** Medium  
**Description:** Add integration tests for git module covering clone, update, and branch operations.

### Issue #1341: Add Docker Modules Integration Tests
**Priority:** Medium  
**Description:** Add integration tests for docker_container and docker_image modules.

---

## 6. Test Matrix for VM Dev Tooling

| Use Case | Required Modules | Test Status |
|----------|-----------------|-------------|
| Install system packages | apt/dnf/pacman/apk | ✅ Tested |
| Install Python packages | pip | ⚠️ Unit only |
| Install Node.js packages | npm | ✅ Tested |
| Install Ruby packages | gem | ✅ Tested |
| Install Rust packages | cargo | ✅ Tested |
| Clone git repositories | git | ⚠️ Unit only |
| Create users | user | ✅ Tested |
| Configure SSH | authorized_key | ✅ Tested |
| Configure services | systemd | ✅ Tested |
| Configure network | netplan/hostname | ✅ Tested |
| Configure timezone | timezone | ✅ Tested |
| Configure locale | locale | ⚠️ Unit only |
| Install Docker containers | docker_container | ⚠️ Unit only |
| Download files | get_url | ⚠️ Unit only |
| Extract archives | unarchive | ⚠️ Unit only |

---

## 7. Conclusion

Rash has excellent coverage for VM dev tooling installation with 101 modules. The key modules for package management, user management, file operations, and service management are well-implemented and tested.

**Key Gaps:**
1. `flatpak` module is missing (medium priority for Fedora/other distros)
2. Some critical modules (apt, pip, git) need integration tests
3. Version manager modules (sdkman, nvm, pyenv) would be nice additions

**Next Steps for @forkline:**
1. Review and create issues for missing modules
2. Prioritize integration test additions
3. Consider flatpak module implementation
