// arm and arm64 tests failed because env doesn't support `-S`:
// `docker run --rm -it --entrypoint env ghcr.io/cross-rs/aarch64-unknown-linux-gnu:0.2.5 --version`
// env (GNU coreutils) 8.25
// And it `-S` support was introduced in coreutils 8.30:
// https://lists.gnu.org/archive/html/info-gnu/2018-07/msg00001.html
#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
mod args;
#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
mod modules;

use std::env;
use std::iter;
use std::path::Path;
use std::process::Command;

pub fn update_path(new_path: &Path) {
    let path = env::var_os("PATH").unwrap();
    let paths = iter::once(new_path.to_path_buf())
        .chain(env::split_paths(&path))
        .collect::<Vec<_>>();
    let new_path = env::join_paths(paths).unwrap();
    unsafe {
        env::set_var("PATH", new_path);
    }
}

pub fn execute_rash(args: &[&str]) -> (String, String) {
    execute_rash_with_env(args, &[])
}

pub fn execute_rash_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> (String, String) {
    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    update_path(bin_path.parent().unwrap());
    let mocks_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mocks");
    update_path(&mocks_path);

    let mut cmd = Command::new(bin_path);
    cmd.args(args);

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
