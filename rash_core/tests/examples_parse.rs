use std::fs;
use std::path::Path;

use rash_core::context::GlobalParams;
use rash_core::task::{parse_file, parse_file_with_handlers};
use walkdir::WalkDir;

#[test]
fn every_rash_example_has_a_valid_task_schema() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let params = GlobalParams::default();
    let mut checked = 0usize;
    let mut failures = Vec::new();

    for entry in WalkDir::new(&examples).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|ext| ext.to_str()) != Some("rh")
        {
            continue;
        }
        checked += 1;
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                failures.push(format!("{}: cannot read: {error}", path.display()));
                continue;
            }
        };

        if parse_file_with_handlers(&content, &params).is_ok()
            || parse_file(&content, &params).is_ok()
        {
            continue;
        }

        let mapping_error = parse_file_with_handlers(&content, &params)
            .unwrap_err()
            .to_string();
        let sequence_error = parse_file(&content, &params).unwrap_err().to_string();
        failures.push(format!(
            "{}:\n  mapping form: {mapping_error}\n  sequence form: {sequence_error}",
            path.display()
        ));
    }

    assert!(
        checked > 0,
        "no .rh examples found under {}",
        examples.display()
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} Rash examples do not parse:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
