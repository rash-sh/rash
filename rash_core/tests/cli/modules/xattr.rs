use crate::cli::modules::run_test;

#[cfg(target_os = "linux")]
#[test]
fn test_xattr_set_and_list() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create test file
  file:
    path: /tmp/xattr_test.txt
    state: touch

- name: Set extended attribute
  xattr:
    path: /tmp/xattr_test.txt
    key: comment
    value: "test comment"

- name: List all xattrs
  xattr:
    path: /tmp/xattr_test.txt
    state: all
  register: xattrs_result

- name: Verify xattr was set
  assert:
    that:
      - xattrs_result.xattrs | length == 1
      - "'comment' in xattrs_result.xattrs[0].key"

- name: Clean up test file
  file:
    path: /tmp/xattr_test.txt
    state: absent
        "#;

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(stderr.is_empty() || !stderr.contains("error"));
}

#[cfg(target_os = "linux")]
#[test]
fn test_xattr_remove() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Create test file
  file:
    path: /tmp/xattr_remove_test.txt
    state: touch

- name: Set extended attribute
  xattr:
    path: /tmp/xattr_remove_test.txt
    key: user.comment
    value: "to be removed"

- name: Remove the attribute
  xattr:
    path: /tmp/xattr_remove_test.txt
    key: user.comment
    state: absent

- name: Verify attribute is gone
  xattr:
    path: /tmp/xattr_remove_test.txt
    state: all
  register: result

- name: Assert no xattrs
  assert:
    that:
      - result.xattrs | length == 0

- name: Clean up
  file:
    path: /tmp/xattr_remove_test.txt
    state: absent
        "#;

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(stderr.is_empty() || !stderr.contains("error"));
}

#[test]
fn test_xattr_path_not_exists() {
    let script_text = r#"
#!/usr/bin/env rash
- name: Try to set xattr on nonexistent file
  xattr:
    path: /nonexistent/path/file.txt
    key: user.comment
    value: "test"
        "#;

    let args: &[&str] = &[];
    let (_stdout, stderr) = run_test(script_text, args);

    assert!(stderr.contains("does not exist"));
}
