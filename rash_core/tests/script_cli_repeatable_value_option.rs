use rash_core::{docopt, script_cli};
use serde_json::json;

fn assert_parity(file: &str, args: &[&str]) {
    let legacy = docopt::parse(file, args);
    let compiled = script_cli::parse(file, args);
    match (legacy, compiled) {
        (Ok(legacy), Ok(compiled)) => assert_eq!(compiled, legacy, "args={args:?}"),
        (Err(legacy), Err(compiled)) => {
            assert_eq!(compiled.kind(), legacy.kind(), "args={args:?}")
        }
        (legacy, compiled) => {
            panic!("parser mismatch for args={args:?}: legacy={legacy:?} compiled={compiled:?}")
        }
    }
}

#[test]
fn repeatable_value_option_preserves_scalar_last_value_semantics() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--tag=<value>]...
#
"#;

    assert_parity(file, &[]);
    assert_parity(file, &["--tag=one"]);
    assert_parity(file, &["--tag=one", "--tag=two"]);
}

#[test]
fn documented_repeatable_value_option_preserves_scalar_last_value_semantics() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--tag]...
#
# Options:
#   --tag=VALUE  tag value
#
"#;

    // Intentional difference: legacy fails to merge the documented value option with the
    // repeated usage reference ("Not mergeable options"). The compiled parser applies the same
    // scalar last-value semantics as `[--tag=<value>]...`.
    assert_eq!(
        script_cli::parse(file, &["--tag", "one"]).unwrap(),
        json!({"options": {"tag": "one"}})
    );
    assert_eq!(
        script_cli::parse(file, &["--tag=one", "--tag=two"]).unwrap(),
        json!({"options": {"tag": "two"}})
    );
}
