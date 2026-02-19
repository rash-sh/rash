use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::task::Task;

use std::collections::{HashMap, HashSet};

use serde_norway::Value as YamlValue;

#[derive(Debug, Clone)]
pub struct Handler<'a> {
    name: String,
    task: Task<'a>,
}

impl<'a> Handler<'a> {
    pub fn new(name: String, task: Task<'a>) -> Self {
        Handler { name, task }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_task(&self) -> &Task<'a> {
        &self.task
    }
}

#[derive(Debug, Clone, Default)]
pub struct Handlers<'a> {
    handlers: HashMap<String, Handler<'a>>,
}

impl<'a> Handlers<'a> {
    pub fn new() -> Self {
        Handlers {
            handlers: HashMap::new(),
        }
    }

    pub fn from_yaml(yaml: &[YamlValue], global_params: &'a GlobalParams) -> Result<Self> {
        let mut handlers = HashMap::new();

        for handler_yaml in yaml {
            let name = handler_yaml
                .get("name")
                .and_then(|n| n.as_str())
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "Handler must have a 'name' attribute",
                    )
                })?
                .to_string();

            let task = Task::new(handler_yaml, global_params)?;
            let handler = Handler::new(name.clone(), task);
            handlers.insert(name, handler);
        }

        Ok(Handlers { handlers })
    }

    pub fn get(&self, name: &str) -> Option<&Handler<'a>> {
        self.handlers.get(name)
    }

    pub fn get_handler_names(&self) -> Vec<&str> {
        self.handlers.keys().map(|s| s.as_str()).collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PendingHandlers {
    pending: HashSet<String>,
}

impl PendingHandlers {
    pub fn new() -> Self {
        PendingHandlers {
            pending: HashSet::new(),
        }
    }

    pub fn notify(&mut self, handler_names: &[String]) {
        for name in handler_names {
            self.pending.insert(name.clone());
        }
    }

    pub fn get_pending(&self) -> &HashSet<String> {
        &self.pending
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn take_pending(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.pending)
    }
}

pub fn parse_notify_value(value: &YamlValue) -> Option<Vec<String>> {
    match value {
        YamlValue::String(s) => Some(vec![s.clone()]),
        YamlValue::Sequence(seq) => Some(
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GlobalParams;

    fn create_test_global_params() -> GlobalParams<'static> {
        GlobalParams::default()
    }

    #[test]
    fn test_parse_notify_value_string() {
        let value = YamlValue::String("handler1".to_string());
        let result = parse_notify_value(&value);
        assert_eq!(result, Some(vec!["handler1".to_string()]));
    }

    #[test]
    fn test_parse_notify_value_sequence() {
        let value = YamlValue::Sequence(vec![
            YamlValue::String("handler1".to_string()),
            YamlValue::String("handler2".to_string()),
        ]);
        let result = parse_notify_value(&value);
        assert_eq!(
            result,
            Some(vec!["handler1".to_string(), "handler2".to_string()])
        );
    }

    #[test]
    fn test_parse_notify_value_invalid() {
        let value = YamlValue::Number(42.into());
        let result = parse_notify_value(&value);
        assert_eq!(result, None);
    }

    #[test]
    fn test_pending_handlers_notify() {
        let mut pending = PendingHandlers::new();
        assert!(pending.is_empty());

        pending.notify(&["handler1".to_string(), "handler2".to_string()]);
        assert!(!pending.is_empty());
        assert!(pending.get_pending().contains("handler1"));
        assert!(pending.get_pending().contains("handler2"));

        pending.notify(&["handler1".to_string()]);
        assert_eq!(pending.get_pending().len(), 2);
    }

    #[test]
    fn test_pending_handlers_take_pending() {
        let mut pending = PendingHandlers::new();
        pending.notify(&["handler1".to_string()]);

        let taken = pending.take_pending();
        assert!(taken.contains("handler1"));
        assert!(pending.is_empty());
    }

    #[test]
    fn test_handlers_from_yaml() {
        let yaml_str = r#"
        - name: restart service
          command: systemctl restart myservice
        - name: reload config
          command: systemctl reload myservice
        "#;
        let handlers_yaml: Vec<YamlValue> = serde_norway::from_str(yaml_str).unwrap();
        let global_params = create_test_global_params();

        let handlers = Handlers::from_yaml(&handlers_yaml, &global_params).unwrap();
        assert!(handlers.get("restart service").is_some());
        assert!(handlers.get("reload config").is_some());
        assert!(handlers.get("nonexistent").is_none());
    }

    #[test]
    fn test_handlers_missing_name() {
        let yaml_str = r#"
        - command: echo test
        "#;
        let handlers_yaml: Vec<YamlValue> = serde_norway::from_str(yaml_str).unwrap();
        let global_params = create_test_global_params();

        let result = Handlers::from_yaml(&handlers_yaml, &global_params);
        assert!(result.is_err());
    }
}
