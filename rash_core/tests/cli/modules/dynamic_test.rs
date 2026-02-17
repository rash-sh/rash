use std::path::PathBuf;

use rash_core::modules::{DynamicModule, DynamicModuleRegistry, Module};

#[test]
fn test_load_dynamic_module() {
    let module_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cli/modules/dynamic/hello_module");
    let module = DynamicModule::load(&module_dir).unwrap();

    assert_eq!(module.get_name(), "hello_module");
}

#[test]
fn test_dynamic_module_registry() {
    let modules_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cli/modules/dynamic");
    let mut registry = DynamicModuleRegistry::with_search_paths(vec![modules_path]);

    assert!(registry.is_dynamic_module("hello_module"));
    assert!(!registry.is_dynamic_module("non_existent_module"));

    let module = registry.load_module("hello_module").unwrap();
    assert_eq!(module.get_name(), "hello_module");
}

#[test]
fn test_dynamic_module_force_string_on_params() {
    let module_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cli/modules/dynamic/hello_module");
    let module = DynamicModule::load(&module_dir).unwrap();

    assert!(!module.force_string_on_params());
}
