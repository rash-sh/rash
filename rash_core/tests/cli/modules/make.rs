use crate::cli::modules::run_test;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_make_default_target() {
    let tmp_dir = tempdir().unwrap();
    let makefile_path = tmp_dir.path().join("Makefile");
    let mut makefile = File::create(&makefile_path).unwrap();
    writeln!(makefile, "all:").unwrap();
    writeln!(makefile, "\techo 'hello from make'").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Run make default target
  make:
    chdir: {}
        "#,
        tmp_dir.path().to_str().unwrap().replace('\\', "\\\\")
    );

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("hello from make"));
}

#[test]
fn test_make_specific_target() {
    let tmp_dir = tempdir().unwrap();
    let makefile_path = tmp_dir.path().join("Makefile");
    let mut makefile = File::create(&makefile_path).unwrap();
    writeln!(makefile, "all:").unwrap();
    writeln!(makefile, "\techo 'default target'").unwrap();
    writeln!(makefile, "install:").unwrap();
    writeln!(makefile, "\techo 'installing...'").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Run make install target
  make:
    chdir: {}
    target: install
        "#,
        tmp_dir.path().to_str().unwrap().replace('\\', "\\\\")
    );

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("installing..."));
    assert!(!stdout.contains("default target"));
}

#[test]
fn test_make_with_params() {
    let tmp_dir = tempdir().unwrap();
    let makefile_path = tmp_dir.path().join("Makefile");
    let mut makefile = File::create(&makefile_path).unwrap();
    writeln!(makefile, "all:").unwrap();
    writeln!(makefile, "\techo $(MODE)").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Run make with params
  make:
    chdir: {}
    params:
      MODE: debug
        "#,
        tmp_dir.path().to_str().unwrap().replace('\\', "\\\\")
    );

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("debug"));
}

#[test]
fn test_make_with_jobs() {
    let tmp_dir = tempdir().unwrap();
    let makefile_path = tmp_dir.path().join("Makefile");
    let mut makefile = File::create(&makefile_path).unwrap();
    writeln!(makefile, "all:").unwrap();
    writeln!(makefile, "\techo 'parallel build'").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Run make with parallel jobs
  make:
    chdir: {}
    jobs: 4
        "#,
        tmp_dir.path().to_str().unwrap().replace('\\', "\\\\")
    );

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("parallel build"));
}

#[test]
fn test_make_with_custom_makefile() {
    let tmp_dir = tempdir().unwrap();
    let custom_makefile_path = tmp_dir.path().join("CustomMakefile");
    let mut makefile = File::create(&custom_makefile_path).unwrap();
    writeln!(makefile, "all:").unwrap();
    writeln!(makefile, "\techo 'custom makefile'").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Run make with custom Makefile
  make:
    chdir: {}
    file: {}
        "#,
        tmp_dir.path().to_str().unwrap().replace('\\', "\\\\"),
        custom_makefile_path.to_str().unwrap().replace('\\', "\\\\")
    );

    let args: &[&str] = &[];
    let (stdout, _stderr) = run_test(&script_text, args);

    assert!(stdout.contains("custom makefile"));
}
