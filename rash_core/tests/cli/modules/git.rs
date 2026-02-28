use std::fs;
use std::path::Path;
use std::process::Command;

use crate::cli::modules::run_test;
use tempfile::tempdir;

fn create_test_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "--allow-empty", "-m", "Initial commit"])
        .current_dir(path)
        .output()
        .unwrap();
}

fn add_commit(path: &Path, msg: &str) {
    Command::new("git")
        .args(["commit", "--allow-empty", "-m", msg])
        .current_dir(path)
        .output()
        .unwrap();
}

#[test]
fn test_git_clone() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Clone repository
  git:
    repo: {}
    dest: {}
    update: false
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
    assert!(clone_dir.join(".git").exists());
}

#[test]
fn test_git_clone_check_mode() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Clone repository in check mode
  git:
    repo: {}
    dest: {}
    update: false
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
    assert!(!clone_dir.exists(), "Clone should not exist in check mode");
}

#[test]
fn test_git_update_no_changes() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);

    Command::new("git")
        .args([
            "clone",
            repo_dir.to_str().unwrap(),
            clone_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Update repository
  git:
    repo: {}
    dest: {}
    update: true
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok': {}",
        stdout
    );
}

#[test]
fn test_git_update_with_changes() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);

    Command::new("git")
        .args([
            "clone",
            repo_dir.to_str().unwrap(),
            clone_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    add_commit(&repo_dir, "Second commit");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Update repository
  git:
    repo: {}
    dest: {}
    update: true
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
}

#[test]
fn test_git_no_update_existing_repo() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);

    Command::new("git")
        .args([
            "clone",
            repo_dir.to_str().unwrap(),
            clone_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    add_commit(&repo_dir, "Second commit");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Do not update repository
  git:
    repo: {}
    dest: {}
    update: false
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("ok"),
        "stdout should contain 'ok': {}",
        stdout
    );
}

#[test]
fn test_git_clone_with_depth() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);
    add_commit(&repo_dir, "Second commit");
    add_commit(&repo_dir, "Third commit");

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Shallow clone repository
  git:
    repo: {}
    dest: {}
    depth: 1
    update: false
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
    assert!(clone_dir.join(".git").exists());
}

#[test]
fn test_git_clone_single_branch() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Clone single branch
  git:
    repo: {}
    dest: {}
    single_branch: true
    update: false
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(stdout.contains("changed"), "stdout: {}", stdout);
}

#[test]
fn test_git_dest_exists_not_repo() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let dest_dir = tmp_dir.path().join("dest");

    create_test_repo(&repo_dir);
    fs::create_dir_all(&dest_dir).unwrap();
    fs::write(dest_dir.join("file.txt"), "content").unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Clone to existing non-git directory
  git:
    repo: {}
    dest: {}
    update: false
        "#,
        repo_dir.to_str().unwrap(),
        dest_dir.to_str().unwrap()
    );

    let args = ["--diff"];
    let (_, stderr) = run_test(&script_text, &args);

    assert!(
        stderr.contains("not a git repository"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn test_git_result_extra() {
    let tmp_dir = tempdir().unwrap();
    let repo_dir = tmp_dir.path().join("source");
    let clone_dir = tmp_dir.path().join("clone");

    create_test_repo(&repo_dir);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Clone repository
  git:
    repo: {}
    dest: {}
    update: false
  register: result
- debug:
    msg: "{{{{ result.extra }}}}"
        "#,
        repo_dir.to_str().unwrap(),
        clone_dir.to_str().unwrap()
    );

    let args = ["--output", "raw"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr: {}", stderr);
    assert!(
        stdout.contains("repo"),
        "stdout should contain 'repo': {}",
        stdout
    );
    assert!(
        stdout.contains("dest"),
        "stdout should contain 'dest': {}",
        stdout
    );
}
