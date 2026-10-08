// arm and arm64 tests failed because env doesn't support `-S`:
// `docker run --rm -it --entrypoint env ghcr.io/cross-rs/aarch64-unknown-linux-gnu:0.2.5 --version`
// env (GNU coreutils) 8.25
// And it `-S` support was introduced in coreutils 8.30:
// https://lists.gnu.org/archive/html/info-gnu/2018-07/msg00001.html
#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
mod args;
#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
mod become_method;
#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
mod environment;
#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
mod modules;
#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
mod process;

use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// PATH for Rash under test: mocks first, then the Rash binary. Passed to each command
/// instead of changing the test process environment, which parallel tests share.
fn test_path() -> OsString {
    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    let mocks_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    let path = env::var_os("PATH").unwrap_or_default();
    let paths = [mocks_path, bin_path.parent().unwrap().to_path_buf()]
        .into_iter()
        .chain(env::split_paths(&path));
    env::join_paths(paths).unwrap()
}

/// Whether the test runs as root, as tests switching users need. Tests that need it are
/// named `test_as_root_*` so CI can run just them as root; otherwise they skip themselves,
/// saying so on stderr (written directly so the test harness does not capture it).
pub fn running_as_root(test: &str) -> bool {
    use std::io::Write;

    let root = nix::unistd::Uid::effective().is_root();
    if !root {
        let _ = writeln!(std::io::stderr(), "{test}: skipped: requires root");
    }
    root
}

pub fn execute_rash(args: &[&str]) -> (String, String) {
    execute_rash_with_env(args, &[])
}

pub fn execute_rash_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> (String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rash"));
    cmd.args(args).env("PATH", test_path());

    // Pass provided environment variables to subprocess
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    dbg!(&stdout);
    dbg!(&stderr);

    (stdout, stderr)
}
