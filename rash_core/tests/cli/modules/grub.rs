use crate::cli::modules::run_test;

#[test]
fn test_grub_configure_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure GRUB
  grub:
    action: configure
    kernel_params:
      - quiet
      - splash
    timeout: 5
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_grub_configure_with_config() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure GRUB with config dict
  grub:
    action: configure
    config:
      GRUB_CMDLINE_LINUX: "root=ZFS=rpool/ROOT/ubuntu"
      GRUB_TIMEOUT: "0"
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_grub_install_bios_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Install GRUB for BIOS
  grub:
    action: install
    device: /dev/sda
    target: i386-pc
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_grub_install_uefi_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Install GRUB for UEFI
  grub:
    action: install
    efi_directory: /boot/efi
    target: x86_64-efi
    removable: true
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_grub_install_missing_device() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Install GRUB without device for BIOS
  grub:
    action: install
    target: i386-pc
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("device is required"));
}

#[test]
fn test_grub_install_missing_efi_directory() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Install GRUB without EFI directory for UEFI
  grub:
    action: install
    target: x86_64-efi
        "#
    .to_string();

    let args = ["--diff"];
    let (_stdout, stderr) = run_test(&script_text, &args);

    assert!(!stderr.is_empty());
    assert!(stderr.contains("efi_directory is required"));
}

#[test]
fn test_grub_update_check_mode() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Update GRUB
  grub:
    action: update
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_grub_configure_serial() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure serial console
  grub:
    action: configure
    terminal: serial
    serial: "--unit=0 --speed=115200"
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_grub_configure_no_changes() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure GRUB with no changes
  grub:
    action: configure
        "#
    .to_string();

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(
        stdout.contains("changed: false")
            || stdout.contains("changed:False")
            || !stdout.contains("changed: true")
    );
}

#[test]
fn test_grub_configure_disable_os_prober() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Disable os-prober
  grub:
    action: configure
    disable_os_prober: true
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}

#[test]
fn test_grub_configure_kernel_params_default() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Configure kernel params default
  grub:
    action: configure
    kernel_params_default:
      - console=tty1
      - console=ttyS0,115200n8
        "#
    .to_string();

    let args = ["--diff", "--check"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty());
    assert!(stdout.contains("changed:"));
}
