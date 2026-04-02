use std::process::Command;

use crate::cli::modules::run_test;

fn kubectl_available() -> bool {
    let output = Command::new("kubectl")
        .args(["cluster-info", "--request-timeout=2s"])
        .output();

    match output {
        Ok(o) => {
            if !o.status.success() {
                return false;
            }
            let stderr = String::from_utf8_lossy(&o.stderr);
            let stdout = String::from_utf8_lossy(&o.stdout);
            if stderr.contains("connection refused")
                || stderr.contains("Unable to connect")
                || stderr.contains("couldn't get")
                || stderr.contains("dial tcp")
                || !stdout.contains("control plane")
            {
                return false;
            }
            true
        }
        Err(_) => false,
    }
}

macro_rules! skip_without_kubectl {
    () => {
        if !kubectl_available() {
            eprintln!("Skipping test: kubectl not available or cluster not accessible");
            return;
        }
    };
}

fn create_namespace(ns: &str) {
    let _ = Command::new("kubectl")
        .args(["create", "namespace", ns])
        .output();
}

fn delete_namespace(ns: &str) {
    let _ = Command::new("kubectl")
        .args(["delete", "namespace", ns, "--ignore-not-found"])
        .output();
}

fn namespace_exists(ns: &str) -> bool {
    Command::new("kubectl")
        .args(["get", "namespace", ns, "--ignore-not-found"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn deployment_exists(ns: &str, name: &str) -> bool {
    Command::new("kubectl")
        .args(["get", "deployment", "-n", ns, name, "--ignore-not-found"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(name))
        .unwrap_or(false)
}

fn delete_deployment(ns: &str, name: &str) {
    let _ = Command::new("kubectl")
        .args(["delete", "deployment", "-n", ns, name, "--ignore-not-found"])
        .output();
}

#[test]
fn test_kubectl_apply_check_mode() {
    skip_without_kubectl!();

    let tmp_dir = tempfile::tempdir().unwrap();
    let manifest_path = tmp_dir.path().join("test-ns.yaml");

    let manifest = r#"
apiVersion: v1
kind: Namespace
metadata:
  name: test-ns-kubectl-check
"#;

    std::fs::write(&manifest_path, manifest).unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not create namespace
  kubectl:
    state: present
    src: {}
"#,
        manifest_path.display()
    );

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );

    assert!(
        !namespace_exists("test-ns-kubectl-check"),
        "Namespace should NOT be created in check mode"
    );
}

#[test]
fn test_kubectl_delete_check_mode() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-delete-check";
    create_namespace(ns);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Check mode - should not delete namespace
  kubectl:
    state: absent
    kind: namespace
    name: {}
"#,
        ns
    );

    let args = ["--check", "--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' in check mode: {}",
        stdout
    );

    assert!(
        namespace_exists(ns),
        "Namespace should NOT be deleted in check mode"
    );

    delete_namespace(ns);
}

#[test]
fn test_kubectl_apply_inline_definition() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-inline";

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Create namespace with inline definition
  kubectl:
    state: present
    definition:
      apiVersion: v1
      kind: Namespace
      metadata:
        name: {}
"#,
        ns
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    assert!(namespace_exists(ns), "Namespace should exist after apply");

    delete_namespace(ns);
}

#[test]
fn test_kubectl_delete_by_kind_name() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-delete-kind";
    create_namespace(ns);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Delete namespace
  kubectl:
    state: absent
    kind: namespace
    name: {}
"#,
        ns
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    assert!(!namespace_exists(ns), "Namespace should be deleted");
}

#[test]
fn test_kubectl_delete_already_absent() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-already-absent";
    delete_namespace(ns);

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Delete non-existent namespace
  kubectl:
    state: absent
    kind: namespace
    name: {}
"#,
        ns
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' for already absent: {}",
        stdout
    );
}

#[test]
fn test_kubectl_apply_with_namespace() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-with-ns";
    create_namespace(ns);

    let tmp_dir = tempfile::tempdir().unwrap();
    let manifest_path = tmp_dir.path().join("deployment.yaml");

    let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-deployment
spec:
  replicas: 1
  selector:
    matchLabels:
      app: test
  template:
    metadata:
      labels:
        app: test
    spec:
      containers:
        - name: test
          image: alpine:latest
          command: ["/bin/sh", "-c", "sleep 10"]
"#;

    std::fs::write(&manifest_path, manifest).unwrap();

    let script_text = format!(
        r#"
#!/usr/bin/env rash
- name: Apply deployment to namespace
  kubectl:
    state: present
    src: {}
    namespace: {}
"#,
        manifest_path.display(),
        ns
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&script_text, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    assert!(
        deployment_exists(ns, "test-deployment"),
        "Deployment should exist"
    );

    delete_deployment(ns, "test-deployment");
    delete_namespace(ns);
}

#[test]
fn test_kubectl_delete_from_manifest() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-delete-manifest";
    create_namespace(ns);

    let tmp_dir = tempfile::tempdir().unwrap();
    let manifest_path = tmp_dir.path().join("deployment.yaml");

    let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-deployment-delete
  namespace: test-ns-kubectl-delete-manifest
spec:
  replicas: 1
  selector:
    matchLabels:
      app: test
  template:
    metadata:
      labels:
        app: test
    spec:
      containers:
        - name: test
          image: alpine:latest
          command: ["/bin/sh", "-c", "sleep 10"]
"#;

    std::fs::write(&manifest_path, manifest).unwrap();

    let apply_script = format!(
        r#"
#!/usr/bin/env rash
- name: Apply deployment first
  kubectl:
    state: present
    src: {}
"#,
        manifest_path.display()
    );

    let (_stdout, stderr) = run_test(&apply_script, &[]);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after apply: {}",
        stderr
    );

    let delete_script = format!(
        r#"
#!/usr/bin/env rash
- name: Delete deployment from manifest
  kubectl:
    state: absent
    src: {}
"#,
        manifest_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&delete_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    assert!(
        !deployment_exists(ns, "test-deployment-delete"),
        "Deployment should be deleted"
    );

    delete_namespace(ns);
}

#[test]
fn test_kubectl_force_delete() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-force-delete";
    create_namespace(ns);

    let tmp_dir = tempfile::tempdir().unwrap();
    let manifest_path = tmp_dir.path().join("pod.yaml");

    let manifest = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod-force
  namespace: test-ns-kubectl-force-delete
spec:
  containers:
    - name: test
      image: alpine:latest
      command: ["/bin/sh", "-c", "sleep 3600"]
"#;

    std::fs::write(&manifest_path, manifest).unwrap();

    let apply_script = format!(
        r#"
#!/usr/bin/env rash
- name: Apply pod first
  kubectl:
    state: present
    src: {}
"#,
        manifest_path.display()
    );

    let (_stdout, stderr) = run_test(&apply_script, &[]);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after apply: {}",
        stderr
    );

    std::thread::sleep(std::time::Duration::from_secs(2));

    let delete_script = format!(
        r#"
#!/usr/bin/env rash
- name: Force delete pod
  kubectl:
    state: absent
    src: {}
    force: true
"#,
        manifest_path.display()
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&delete_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed': {}",
        stdout
    );

    delete_namespace(ns);
}

#[test]
fn test_kubectl_scale_deployment() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-scale";
    create_namespace(ns);

    let tmp_dir = tempfile::tempdir().unwrap();
    let manifest_path = tmp_dir.path().join("deployment.yaml");

    let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-deployment-scale
spec:
  replicas: 1
  selector:
    matchLabels:
      app: test
  template:
    metadata:
      labels:
        app: test
    spec:
      containers:
        - name: test
          image: alpine:latest
          command: ["/bin/sh", "-c", "sleep 3600"]
"#;

    std::fs::write(&manifest_path, manifest).unwrap();

    let apply_script = format!(
        r#"
#!/usr/bin/env rash
- name: Apply deployment with namespace
  kubectl:
    state: present
    src: {}
    namespace: {}
"#,
        manifest_path.display(),
        ns
    );

    let (_stdout, stderr) = run_test(&apply_script, &[]);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after apply: {}",
        stderr
    );

    std::thread::sleep(std::time::Duration::from_secs(2));

    let scale_script = format!(
        r#"
#!/usr/bin/env rash
- name: Scale deployment to 3 replicas
  kubectl:
    state: present
    kind: deployment
    name: test-deployment-scale
    namespace: {}
    replicas: 3
"#,
        ns
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&scale_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        stdout.contains("changed"),
        "stdout should contain 'changed' when scaling: {}",
        stdout
    );

    delete_deployment(ns, "test-deployment-scale");
    delete_namespace(ns);
}

#[test]
fn test_kubectl_scale_already_correct() {
    skip_without_kubectl!();

    let ns = "test-ns-kubectl-scale-already";
    create_namespace(ns);

    let tmp_dir = tempfile::tempdir().unwrap();
    let manifest_path = tmp_dir.path().join("deployment.yaml");

    let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-deployment-already
spec:
  replicas: 3
  selector:
    matchLabels:
      app: test
  template:
    metadata:
      labels:
        app: test
    spec:
      containers:
        - name: test
          image: alpine:latest
          command: ["/bin/sh", "-c", "sleep 3600"]
"#;

    std::fs::write(&manifest_path, manifest).unwrap();

    let apply_script = format!(
        r#"
#!/usr/bin/env rash
- name: Apply deployment with 3 replicas
  kubectl:
    state: present
    src: {}
    namespace: {}
"#,
        manifest_path.display(),
        ns
    );

    let (_stdout, stderr) = run_test(&apply_script, &[]);
    assert!(
        stderr.is_empty(),
        "stderr should be empty after apply: {}",
        stderr
    );

    std::thread::sleep(std::time::Duration::from_secs(2));

    let scale_script = format!(
        r#"
#!/usr/bin/env rash
- name: Scale deployment to same replica count
  kubectl:
    state: present
    kind: deployment
    name: test-deployment-already
    namespace: {}
    replicas: 3
"#,
        ns
    );

    let args = ["--diff"];
    let (stdout, stderr) = run_test(&scale_script, &args);

    assert!(stderr.is_empty(), "stderr should be empty: {}", stderr);
    assert!(
        !stdout.contains("changed"),
        "stdout should not contain 'changed' when already at target replicas: {}",
        stdout
    );

    delete_deployment(ns, "test-deployment-already");
    delete_namespace(ns);
}
