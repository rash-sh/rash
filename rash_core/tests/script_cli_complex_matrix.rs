use rash_core::{docopt, script_cli};

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
fn complex_option_clusters_and_defaults_match_legacy() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool [options] <file>
#
# Options:
#   -v, --verbose            Show detailed output
#   -f, --format=<format>    Output format [default: json]
#   -r, --repeat=<n>         Repeat operation n times [default: 1]
#   -q, --quiet              Suppress output
#
"#;

    assert_parity(file, &["-vfyaml", "-r5", "-q", "data.txt"]);
    assert_parity(file, &["--format=xml", "--repeat=3", "data.txt"]);
    assert_parity(file, &["--format", "yaml", "data.txt"]);
}

#[test]
fn nested_groups_with_repeatable_positionals_match_legacy() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool [options] (add [<item>...] | (remove|delete) <id>)
#
# Options:
#   -f, --force    Force operation
#
"#;

    assert_parity(file, &["--force", "add", "item1", "item2", "item3"]);
    assert_parity(file, &["add"]);
    assert_parity(file, &["remove", "12345"]);
    assert_parity(file, &["delete", "12345"]);
}

#[test]
fn combined_multi_usage_patterns_match_legacy() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool deploy [--env=<environment>] [--dry-run] [<service>...]
#   ./tool rollback [--force] <version>
#   ./tool (start|stop|restart) [(--all | <service>...)]
#
# Options:
#   --env=<environment>  Target environment [default: dev]
#   --dry-run            Don't actually deploy
#   --force              Force the operation
#   --all                Apply to all services
#
"#;

    for args in [
        vec!["deploy", "--env=prod", "--dry-run", "web", "api", "db"],
        vec!["deploy"],
        vec!["rollback", "--force", "v1.2.3"],
        vec!["start", "web", "api"],
        vec!["start", "--all"],
        vec!["restart"],
    ] {
        assert_parity(file, &args);
    }
}

#[test]
fn mixed_required_optional_and_alternative_groups_match_legacy() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool sync (<source> <dest>) [--delete]
#   ./tool query [--format=<format>] (<key> | --all)
#
# Options:
#   --delete           Delete files missing from source
#   --format=<format>  Output format [default: text]
#   --all              Query all values
#
"#;

    assert_parity(file, &["sync", "src", "dst"]);
    assert_parity(file, &["sync", "src", "dst", "--delete"]);
    assert_parity(file, &["query", "foo"]);
    assert_parity(file, &["query", "--format=json", "foo"]);
    assert_parity(file, &["query", "--all"]);
}
