// arm and arm64 tests failed because env doesn't support `-S`:
// `docker run --rm -it --entrypoint env ghcr.io/cross-rs/aarch64-unknown-linux-gnu:0.2.5 --version`
// env (GNU coreutils) 8.25
// And it `-S` support was introduced in coreutils 8.30:
// https://lists.gnu.org/archive/html/info-gnu/2018-07/msg00001.html
#[cfg(all(not(target_arch = "aarch64"), not(target_arch = "arm")))]
mod include;
#[cfg(all(not(target_arch = "aarch64"), not(target_arch = "arm")))]
mod pacman;

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Write;
use std::iter;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

pub fn update_path(new_path: &Path) {
    let path = env::var_os("PATH").unwrap();
    let paths = iter::once(new_path.to_path_buf())
        .chain(env::split_paths(&path))
        .collect::<Vec<_>>();
    let new_path = env::join_paths(paths).unwrap();
    env::set_var("PATH", new_path);
}

pub fn run_tests(
    scripts: HashMap<&str, &str>,
    entrypoint: &str,
    args: &[&str],
) -> (String, String) {
    let tmp_dir = tempdir().unwrap();

    scripts.into_iter().for_each(|(name, content)| {
        let script_path = tmp_dir.path().join(name);
        let mut script_file = File::create(&script_path).unwrap();
        script_file.write_all(content.as_bytes()).unwrap();
    });

    let bin_path = Path::new(env!("CARGO_BIN_EXE_rash"));
    update_path(bin_path.parent().unwrap());

    let entrypoint_path = tmp_dir.path().join(entrypoint);

    let mut cmd = Command::new(bin_path);
    cmd.args(args);
    cmd.arg(entrypoint_path);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    dbg!(&stdout);
    dbg!(&stderr);

    (stdout, stderr)
}

pub fn run_test(content: &str, args: &[&str]) -> (String, String) {
    let entrypoint = "script.rh";
    let scripts = HashMap::from([(entrypoint, content)]);
    run_tests(scripts, entrypoint, args)
}
