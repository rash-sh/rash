//! Compiled script CLI parser (`script_cli`).
//!
//! Every benchmark measures a full `parse` call: the public API does not expose compilation and
//! matching separately, so `script_cli_compile_dominated` (large declaration, empty argv) and
//! `script_cli_arguments` (tiny declaration, long argv) bracket the two costs.
//!
//! Suggested run: `cargo bench -p rash_core --bench script_cli -- --warm-up-time 1 --measurement-time 3`.

use std::hint::black_box;
use std::time::Duration;

use criterion::{
    AxisScale, BenchmarkGroup, BenchmarkId, Criterion, PlotConfiguration, Throughput,
    criterion_group, criterion_main, measurement::WallTime,
};

use rash_core::script_cli;

const NAVAL_FATE: &str = r#"
#!/usr/bin/env rash
#
# Naval Fate.
#
# Usage:
#   naval_fate.rh ship new <name>...
#   naval_fate.rh ship <name> move <x> <y> [--speed=<kn>]
#   naval_fate.rh ship shoot <x> <y>
#   naval_fate.rh mine (set|remove) <x> <y> [--moored|--drifting]
#   naval_fate.rh -h | --help
#   naval_fate.rh --version
#
# Options:
#   -h --help        Show this screen.
#   -v --version     Show version.
#   -s --speed=<kn>  Speed in knots [default: 10].
#   --moored         Moored (anchored) mine.
#   --drifting       Drifting mine.
#
"#;

/// Benchmark `file`/`args`, checking first that parsing succeeds or fails as expected.
fn bench_parse(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    file: &str,
    args: &[&str],
    expect_ok: bool,
) {
    assert_eq!(
        script_cli::parse(file, args).is_ok(),
        expect_ok,
        "{name} args={args:?}"
    );
    group.bench_with_input(BenchmarkId::from_parameter(name), args, |b, args| {
        b.iter(|| script_cli::parse(black_box(file), black_box(args)))
    });
}

fn run_small_scripts(c: &mut Criterion) {
    let mut group = c.benchmark_group("script_cli_small");
    let cases: [(&str, &[&str], bool); 4] = [
        ("success-new", &["ship", "new", "titanic"], true),
        (
            "success-move",
            &["ship", "titanic", "move", "1", "2", "--speed=20"],
            true,
        ),
        ("failure-no-match", &["ship", "titanic", "sink"], false),
        ("failure-unknown-option", &["mine", "--bogus"], false),
    ];
    for (name, args, expect_ok) in cases {
        bench_parse(&mut group, name, NAVAL_FATE, args, expect_ok);
    }
    group.finish();
}

fn run_ambiguous_grammars(c: &mut Criterion) {
    let overlapping = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;
    let ambiguous = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool <source> <dest>
#   tool <input> <output>
#
"#;
    let mut group = c.benchmark_group("script_cli_ambiguous");
    // Several successful paths with identical bindings.
    bench_parse(
        &mut group,
        "overlapping-identical",
        overlapping,
        &["a", "b", "c", "d", "/tmp"],
        true,
    );
    // Different bindings: the declaration is rejected as ambiguous.
    bench_parse(
        &mut group,
        "different-bindings",
        ambiguous,
        &["a", "b"],
        false,
    );
    group.finish();
}

fn run_repeated_options(c: &mut Criterion) {
    let options =
        "#\n# Options:\n#   -v --verbose  more\n#   -q --quiet    less\n#   --tag=<tag>   tag\n#\n";
    let counted = format!("\n#\n# Usage: tool [--verbose]... <file>\n{options}");
    let shortcut = format!("\n#\n# Usage: tool [options] <file>\n{options}");
    let tags = format!("\n#\n# Usage: tool [--tag=<tag>]... <file>\n{options}");
    let flags_100: Vec<&str> = std::iter::repeat_n("-v", 100).chain(["file"]).collect();
    let mixed_100: Vec<&str> = std::iter::repeat_n(["-v", "--quiet"], 50)
        .flatten()
        .chain(["file"])
        .collect();
    let tags_50: Vec<&str> = std::iter::repeat_n("--tag=x", 50).chain(["file"]).collect();
    let cases: [(&str, &str, &[&str]); 5] = [
        ("counter-cluster-vvvv", &counted, &["-vvvv", "file"]),
        ("counter-flags-100", &counted, &flags_100),
        (
            "shortcut-cluster-vvvv-qq",
            &shortcut,
            &["-vvvv", "-qq", "file"],
        ),
        ("shortcut-mixed-100", &shortcut, &mixed_100),
        ("value-option-50", &tags, &tags_50),
    ];
    let mut group = c.benchmark_group("script_cli_repeated_options");
    for (name, file, args) in cases {
        bench_parse(&mut group, name, file, args, true);
    }
    group.finish();
}

fn run_compile_dominated(c: &mut Criterion) {
    let mut group = c.benchmark_group("script_cli_compile_dominated");
    bench_parse(&mut group, "pacman-empty-argv", PACMAN, &[], true);
    group.finish();
}

fn run_arguments(c: &mut Criterion) {
    let file = r#"
#Naval Fate.
#
# Usage:
#   bench <name>...
    "#;

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
    let mut group = c.benchmark_group("script_cli_arguments");
    group.plot_config(plot_config);

    for args_len in [10, 100, 1000, 10000] {
        let values: Vec<String> = (0..args_len).map(|i| format!("value-{i}")).collect();
        let args: Vec<&str> = values.iter().map(String::as_str).collect();
        group.throughput(Throughput::Elements(args_len as u64));
        bench_parse(&mut group, &args_len.to_string(), file, &args, true);
    }
    group.finish();
}

const PACMAN: &str = r#"
# Pacman binary mock for Pacman module tests.
#
# Usage:
#   ./pacman.rh [options] [<packages>]...
#
# Options:
#  -b, --dbpath <path>  set an alternate database location
#  -c, --clean          remove old packages from cache directory (-cc for all)
#  -d, --nodeps         skip dependency version checks (-dd to skip all checks)
#  -g, --groups         view all members of a package group
#                       (-gg to view all groups and members)
#  -i, --info           view package information (-ii for extended information)
#  -l, --list <repo>    view a list of packages in a repo
#  -p, --print          print the targets instead of performing the operation
#  -q, --quiet          show less information for query and search
#  -r, --root <path>    set an alternate installation root
#  -s, --search <regex> search remote repositories for matching strings
#  -u, --sysupgrade     upgrade installed packages (-uu enables downgrades)
#  -v, --verbose        be verbose
#  -w, --downloadonly   download packages but do not install/upgrade anything
#  -y, --refresh        download fresh package databases from the server
#                       (-yy to force a refresh even if up to date)
#      --arch <arch>    set an alternate architecture
#      --asdeps         install packages as non-explicitly installed
#      --asexplicit     install packages as explicitly installed
#      --assume-installed <package=version>
#                       add a virtual package to satisfy dependencies
#      --cachedir <dir> set an alternate package cache location
#      --color <when>   colourise the output
#      --config <path>  set an alternate configuration file
#      --confirm        always ask for confirmation
#      --dbonly         only modify database entries, not package files
#      --debug          display debug messages
#      --disable-download-timeout
#                       use relaxed timeouts for download
#      --gpgdir <path>  set an alternate home directory for GnuPG
#      --hookdir <dir>  set an alternate hook location
#      --ignore <pkg>   ignore a package upgrade (can be used more than once)
#      --ignoregroup <grp>
#                       ignore a group upgrade (can be used more than once)
#      --logfile <path> set an alternate log file
#      --needed         do not reinstall up to date packages
#      --noconfirm      do not ask for any confirmation
#      --noprogressbar  do not show a progress bar when downloading files
#      --noscriptlet    do not execute the install scriptlet if one exists
#      --overwrite <glob>
#                       overwrite conflicting files (can be used more than once)
#      --print-format <string>
#                       specify how the targets should be printed
#      --sysroot        operate on a mounted guest system (root-only)
#      --help
"#;

fn run_options(c: &mut Criterion) {
    let mut group = c.benchmark_group("script_cli_options");
    let args = vec![
        "-b",
        "yea",
        "-cdgi",
        "-l",
        "boo",
        "-p",
        "-q",
        "-r",
        "yea",
        "-s",
        "boo",
        "-yvwy",
        "--arch",
        "yea",
        "--asdeps",
        "--asexplicit",
        "--assume-installed",
        "yea",
        "--cachedir=boo",
        "--color",
        "yea",
        "--config",
        "ye",
        "--confirm",
        "--dbonly",
        "--debug",
        "--disable-download-timeout",
        "--gpgdir",
        "gooo",
        "--hookdir",
        "assa",
        "--ignore",
        "yea",
        "--ignoregroup=yea",
        "--logfile=boo",
        "--needed",
        "--noconfirm",
        "--noprogressbar",
        "--noscriptlet",
        "--overwrite",
        "yea",
        "--print-format",
        "yea",
        "--sysroot",
    ];

    bench_parse(&mut group, "pacman", PACMAN, &args, true);
    group.finish();
}

fn run_optional_option_scaling(c: &mut Criterion) {
    let cases = [
        (
            "8-options",
            r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <target>
#
# Options:
#   -a --alpha    alpha
#   -b --beta     beta
#   -c --charlie  charlie
#   -d --delta    delta
#   -e --echo     echo
#   -f --foxtrot  foxtrot
#   -g --golf     golf
#   -h --hotel    hotel
#
"#,
            vec!["--hotel", "--alpha", "--golf", "--charlie", "target"],
        ),
        (
            "16-options",
            r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <target>
#
# Options:
#   -a --alpha     alpha
#   -b --beta      beta
#   -c --charlie   charlie
#   -d --delta     delta
#   -e --echo      echo
#   -f --foxtrot   foxtrot
#   -g --golf      golf
#   -h --hotel     hotel
#   -i --india     india
#   -j --juliet    juliet
#   -k --kilo      kilo
#   -l --lima      lima
#   -m --mike      mike
#   -n --november  november
#   -o --oscar     oscar
#   -p --papa      papa
#
"#,
            vec![
                "--papa",
                "--alpha",
                "--november",
                "--charlie",
                "--lima",
                "--echo",
                "--hotel",
                "--juliet",
                "target",
            ],
        ),
    ];

    let mut group = c.benchmark_group("script_cli_optional_option_scaling");
    for (name, file, args) in cases {
        bench_parse(&mut group, name, file, &args, true);
    }
    group.finish();
}

fn run_nested_alternatives(c: &mut Criterion) {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool ((start | stop) (api | worker) (fast | safe)) [--force] <target>
#
# Options:
#   --force  force
#
"#;
    let args = vec!["start", "worker", "safe", "--force", "node"];
    let mut group = c.benchmark_group("script_cli_nested_alternatives");
    bench_parse(&mut group, "start-worker-safe", file, &args, true);
    group.finish();
}

criterion_group!(name = script_cli_benches;
    config = Criterion::default()
    .sample_size(10)
    .warm_up_time(Duration::from_secs(1))
    .measurement_time(Duration::from_secs(3))
    .with_plots();
    targets = run_small_scripts, run_compile_dominated, run_ambiguous_grammars,
        run_repeated_options, run_arguments, run_options,
        run_optional_option_scaling, run_nested_alternatives);
criterion_main!(script_cli_benches);
