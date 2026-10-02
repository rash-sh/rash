use minijinja::{Value, context};
use rash_core::context::GlobalParams;
use rash_core::task::{Task, parse_file};
use serde_norway::Value as YamlValue;

fn task_from_yaml<'a>(yaml: &str, params: &'a GlobalParams<'a>) -> Task<'a> {
    let value: YamlValue = serde_norway::from_str(yaml).unwrap();
    Task::new(&value, params).unwrap()
}

fn registered(result: &rash_core::task::TaskExecResult, name: &str) -> Value {
    result
        .get_vars()
        .unwrap_or_else(|| panic!("task did not register {name}"))
        .get_attr(name)
        .unwrap_or_else(|_| panic!("registered value {name} not found"))
}

#[test]
fn normal_nonzero_exit_preserves_structured_result() {
    let params = GlobalParams::default();
    let task = task_from_yaml(
        r#"
        command:
          argv: [sh, -c, "printf normal; printf problem >&2; exit 7"]
        register: probe
        ignore_errors: true
        changed_when: false
        "#,
        &params,
    );

    let result = task.exec(context! {}).unwrap();
    assert!(result.get_failed());
    assert!(!result.get_changed());
    let probe = registered(&result, "probe");
    assert_eq!(probe.get_attr("rc").unwrap().as_i64(), Some(7));
    assert_eq!(probe.get_attr("output").unwrap().as_str(), Some("normal"));
    assert_eq!(probe.get_attr("stdout").unwrap().as_str(), Some("normal"));
    assert!(probe.get_attr("failed").unwrap().is_true());
    assert!(
        probe
            .get_attr("stderr")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("problem")
    );
}

#[test]
fn failed_when_can_turn_success_into_semantic_failure() {
    let params = GlobalParams::default();
    let task = task_from_yaml(
        r#"
        command:
          argv: [sh, -c, "exit 0"]
        register: probe
        failed_when: true
        ignore_errors: true
        "#,
        &params,
    );

    let result = task.exec(context! {}).unwrap();
    assert!(result.get_failed());
    let probe = registered(&result, "probe");
    assert_eq!(probe.get_attr("rc").unwrap().as_i64(), Some(0));
    assert!(probe.get_attr("failed").unwrap().is_true());
}

#[test]
fn sequential_loop_uses_same_failure_contract() {
    let params = GlobalParams::default();
    let task = task_from_yaml(
        r#"
        command:
          argv: [sh, -c, "exit {{ item }}"]
        loop: [5]
        register: loop_result
        ignore_errors: true
        "#,
        &params,
    );

    let result = task.exec(context! {}).unwrap();
    assert!(result.get_failed());
    let probe = registered(&result, "loop_result");
    assert_eq!(probe.get_attr("rc").unwrap().as_i64(), Some(5));
    assert!(probe.get_attr("failed").unwrap().is_true());
}

#[test]
fn loop_failure_triggers_one_rescue_for_the_aggregate_task() {
    let params = GlobalParams::default();
    let task = task_from_yaml(
        r#"
        command:
          argv: [sh, -c, "exit {{ item }}"]
        loop: [4]
        rescue:
          - set_vars:
              aggregate_rescued: true
        always:
          - set_vars:
              cleanup_ran: true
        "#,
        &params,
    );

    let result = task.exec(context! {}).unwrap();
    assert!(!result.get_failed());
    let vars = result.get_vars().unwrap();
    assert!(vars.get_attr("aggregate_rescued").unwrap().is_true());
    assert!(vars.get_attr("cleanup_ran").unwrap().is_true());
}

#[test]
fn async_nonzero_exit_can_be_reclassified_by_failed_when() {
    let params = GlobalParams::default();
    let task = task_from_yaml(
        r#"
        command:
          argv: [sh, -c, "printf async; printf async-error >&2; exit 9"]
        async: 10
        poll: 1
        register: async_result
        failed_when: false
        changed_when: false
        "#,
        &params,
    );

    let result = task.exec(context! {}).unwrap();
    assert!(!result.get_failed());
    assert!(!result.get_changed());
    let probe = registered(&result, "async_result");
    assert_eq!(probe.get_attr("rc").unwrap().as_i64(), Some(9));
    assert!(!probe.get_attr("failed").unwrap().is_true());
    assert_eq!(probe.get_attr("stdout").unwrap().as_str(), Some("async"));
    assert!(
        probe
            .get_attr("stderr")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("async-error")
    );
}

#[test]
fn explicit_exit_from_rescue_still_runs_outer_always() {
    let params = GlobalParams::default();
    let task = task_from_yaml(
        r#"
        command: false
        rescue:
          - meta:
              action: exit
              code: 31
        always:
          - command:
              argv: [sh, -c, "printf cleanup > /tmp/rash-explicit-exit-always-test"]
            changed_when: false
        "#,
        &params,
    );

    let _ = std::fs::remove_file("/tmp/rash-explicit-exit-always-test");
    let error = task.exec(context! {}).unwrap_err();
    assert_eq!(error.kind(), rash_core::error::ErrorKind::ExplicitExit);
    assert_eq!(error.raw_os_error(), Some(31));
    assert_eq!(
        std::fs::read_to_string("/tmp/rash-explicit-exit-always-test").unwrap(),
        "cleanup"
    );
    let _ = std::fs::remove_file("/tmp/rash-explicit-exit-always-test");
}

#[test]
fn hard_rescue_failure_still_runs_outer_always() {
    let params = GlobalParams::default();
    let task = task_from_yaml(
        r#"
        command: false
        rescue:
          - fail:
              msg: rescue failed hard
        always:
          - command:
              argv: [sh, -c, "printf cleanup > /tmp/rash-rescue-failure-always-test"]
            changed_when: false
        "#,
        &params,
    );

    let _ = std::fs::remove_file("/tmp/rash-rescue-failure-always-test");
    let error = task.exec(context! {}).unwrap_err();
    assert!(error.to_string().contains("rescue failed hard"));
    assert_eq!(
        std::fs::read_to_string("/tmp/rash-rescue-failure-always-test").unwrap(),
        "cleanup"
    );
    let _ = std::fs::remove_file("/tmp/rash-rescue-failure-always-test");
}

#[test]
fn script_defaults_apply_to_multiple_tasks_and_merge_environment() {
    let params = GlobalParams::default();
    let script = r#"
    defaults:
      changed_when: false
      environment:
        FROM_DEFAULT: yes
        OVERRIDE: default
    tasks:
      - command:
          argv: [sh, -c, "test \"$FROM_DEFAULT\" = yes && test \"$OVERRIDE\" = default"]
        register: first
      - command:
          argv: [sh, -c, "test \"$FROM_DEFAULT\" = yes && test \"$OVERRIDE\" = task"]
        environment:
          OVERRIDE: task
        register: second
    "#;
    let parsed = rash_core::task::parse_file_with_handlers(script, &params).unwrap();
    assert_eq!(parsed.tasks.len(), 2);

    let first = parsed.tasks[0].exec(context! {}).unwrap();
    assert!(!first.get_changed());
    assert!(!first.get_failed());

    let second = parsed.tasks[1].exec(context! {}).unwrap();
    assert!(!second.get_changed());
    assert!(!second.get_failed());
}

#[test]
fn traditional_sequence_script_form_remains_valid() {
    let params = GlobalParams::default();
    let tasks = parse_file("- debug: { msg: compatible }", &params).unwrap();
    assert_eq!(tasks.len(), 1);
}
