use rash_core::{docopt, error::ErrorKind, script_cli};

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
fn duplicate_short_alias_keeps_both_unambiguous_long_options() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -u --sysupgrade  upgrade
#   -u --upgrades    list upgrades
#
"#;

    assert_parity(file, &["--sysupgrade"]);
    assert_parity(file, &["--upgrades"]);

    let error = script_cli::parse(file, &["-u"]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(error.to_string().contains("Ambiguous option alias: -u"));
}

#[test]
fn real_pacman_fixture_parses_unambiguous_long_aliases() {
    let file = include_str!("mocks/pacman.rh");
    assert_parity(file, &["--sysupgrade"]);
    assert_parity(file, &["--upgrades"]);
    assert_parity(file, &["--sync", "--noconfirm", "rash"]);
}
