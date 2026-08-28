use std::time::Duration;

use criterion::{
    AxisScale, BenchmarkId, Criterion, PlotConfiguration, Throughput, criterion_group,
    criterion_main,
};

use rash_core::{docopt, script_cli};

fn run_docopt_arguments(c: &mut Criterion) {
    let file = r#"
#Naval Fate.
#
# Usage:
#   bench <name>...
    "#;

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
    let mut group = c.benchmark_group("script_cli_arguments");
    group.measurement_time(Duration::from_secs(25));
    group.plot_config(plot_config);

    for args_len in [10, 100, 1000, 10000] {
        let args = vec!["foo"; args_len];
        group.throughput(Throughput::Elements(args_len as u64));
        group.bench_with_input(BenchmarkId::new("legacy", args_len), &args, |b, args| {
            b.iter(|| docopt::parse(file, args).unwrap());
        });
        group.bench_with_input(BenchmarkId::new("compiled", args_len), &args, |b, args| {
            b.iter(|| script_cli::parse(file, args).unwrap());
        });
    }
    group.finish();
}

fn run_docopt_options(c: &mut Criterion) {
    let file = r#"
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
    let mut group = c.benchmark_group("script_cli_options");
    group.measurement_time(Duration::from_secs(25));
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

    group.bench_with_input(BenchmarkId::new("legacy", "pacman"), &args, |b, args| {
        b.iter(|| docopt::parse(file, args).unwrap());
    });
    group.bench_with_input(BenchmarkId::new("compiled", "pacman"), &args, |b, args| {
        b.iter(|| script_cli::parse(file, args).unwrap());
    });
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
        group.bench_with_input(BenchmarkId::new("legacy", name), &args, |b, args| {
            b.iter(|| docopt::parse(file, args).unwrap());
        });
        group.bench_with_input(BenchmarkId::new("compiled", name), &args, |b, args| {
            b.iter(|| script_cli::parse(file, args).unwrap());
        });
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
    group.bench_function("legacy", |b| {
        b.iter(|| docopt::parse(file, &args).unwrap());
    });
    group.bench_function("compiled", |b| {
        b.iter(|| script_cli::parse(file, &args).unwrap());
    });
    group.finish();
}

criterion_group!(name = docopt;
    config = Criterion::default()
    .sample_size(10)
    .warm_up_time(Duration::from_secs(3))
    .with_plots();
    targets = run_docopt_arguments, run_docopt_options, run_optional_option_scaling, run_nested_alternatives);
criterion_main!(docopt);
