use std::fs::{self, File};
use std::io::Write;

use tempfile::tempdir;

use super::execute_rash_with_env;

fn setup_hello_module(tmp_dir: &tempfile::TempDir) {
    let module_dir = tmp_dir.path().join("modules").join("hello_module");
    fs::create_dir_all(&module_dir).unwrap();

    let meta_content = r#"
name: hello_module
description: A simple hello world module
params:
  message:
    type: string
    required: true
    description: The message to display
  prefix:
    type: string
    required: false
    default: "Hello: "
"#;
    let mut meta_file = File::create(module_dir.join("meta.yml")).unwrap();
    meta_file.write_all(meta_content.as_bytes()).unwrap();

    let main_content = r#"
- name: Set default changed
  set_vars:
    __module_changed: false

- name: Display message
  debug:
    msg: "{{ module.params.prefix }}{{ module.params.message }}"

- name: Set output
  set_vars:
    __module_output: "{{ module.params.prefix }}{{ module.params.message }}"
"#;
    let mut main_file = File::create(module_dir.join("main.yml")).unwrap();
    main_file.write_all(main_content.as_bytes()).unwrap();
}

fn run_with_hello_module(script: &str) -> (String, String) {
    let tmp_dir = tempdir().unwrap();
    setup_hello_module(&tmp_dir);

    let script_path = tmp_dir.path().join("script.rh");
    let mut script_file = File::create(&script_path).unwrap();
    script_file.write_all(script.as_bytes()).unwrap();

    let args = vec![script_path.to_str().unwrap()];
    execute_rash_with_env(&args, &[])
}

#[test]
fn test_dynamic_module_hello() {
    let script = r#"
- name: Use hello module
  hello_module:
    message: "World"
  register: result

- name: Verify result exists
  debug:
    msg: "Result: {{ result }}"
"#;

    let (stdout, stderr) = run_with_hello_module(script);
    assert!(stderr.is_empty(), "stderr should be empty, got: {stderr}");
    assert!(
        stdout.contains("Hello: World"),
        "stdout should contain 'Hello: World', got: {stdout}"
    );
}

#[test]
fn test_dynamic_module_with_custom_prefix() {
    let script = r#"
- name: Use hello module with custom prefix
  hello_module:
    message: "Rust"
    prefix: "Hi_"
  register: result

- name: Verify output
  debug:
    msg: "Output: {{ result.output }}"
"#;

    let (stdout, stderr) = run_with_hello_module(script);
    assert!(stderr.is_empty(), "stderr should be empty, got: {stderr}");
    assert!(
        stdout.contains("Hi_Rust"),
        "stdout should contain 'Hi_Rust', got: {stdout}"
    );
}

#[test]
fn test_dynamic_module_missing_required_param() {
    let script = r#"
- name: Use hello module without required param
  hello_module: {}
"#;

    let (_stdout, stderr) = run_with_hello_module(script);
    assert!(
        stderr.contains("Required parameter") || stderr.contains("error"),
        "stderr should contain error about missing param, got: {stderr}"
    );
}
