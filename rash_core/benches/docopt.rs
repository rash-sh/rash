use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion, PlotConfiguration,
    Throughput,
};

use rash_core::docopt::parse;

fn run_docopt_arguments(c: &mut Criterion) {
    let file = r#"
#Naval Fate.
#
# Usage:
#   bench <name>...
    "#;

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);

    let mut group = c.benchmark_group("run_docopt_arguments");
    group.measurement_time(Duration::from_secs(25));
    group.plot_config(plot_config);

    for args_len in [10, 100, 1000, 10000].iter() {
        let args = vec!["foo"; args_len.to_owned()];
        group.throughput(Throughput::Elements(*args_len as u64));
        group.bench_with_input(BenchmarkId::from_parameter(args_len), &args, |b, args| {
            b.iter(|| parse(file, args).unwrap());
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
    let mut group = c.benchmark_group("run_docopt_options");

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
        "--cachedir",
        "boo",
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
        "--ignoregroup",
        "yea",
        "--logfile",
        "boo",
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
    group.bench_with_input(
        BenchmarkId::from_parameter("all options"),
        &args,
        |b, args| {
            b.iter(|| parse(file, args).unwrap());
        },
    );

    group.finish();
}

criterion_group!(name = docopt;
    config = Criterion::default()
    .sample_size(10)
    .warm_up_time(Duration::from_secs(3))
    .with_plots();
    targets = run_docopt_arguments, run_docopt_options);
criterion_main!(docopt);
