use crate::error::{Error, ErrorKind, Result};
use crate::modules::is_module;
use crate::task::Task;
use crate::task::valid::TaskValid;

use serde_yaml::Value;

/// TaskNew is a new task without a verified Yaml
#[derive(Debug)]
pub struct TaskNew {
    proto_attrs: Value,
}

impl From<&Value> for TaskNew {
    fn from(yaml: &Value) -> Self {
        TaskNew {
            proto_attrs: yaml.clone(),
        }
    }
}

impl TaskNew {
    /// Validate all `proto_attrs` which can be represented as String and are task fields or modules
    pub fn validate_attrs(&self) -> Result<TaskValid> {
        let proto_attrs_copy = self.proto_attrs.clone();
        let attrs_map = proto_attrs_copy.as_mapping().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Task is not a mapping {:?}", self.proto_attrs),
            )
        })?;
        let attrs_seq = attrs_map
            .iter()
            .map(|(key, _)| {
                key.clone().as_str().map(String::from).ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("{:?} is not valid in {:?}", key, self.proto_attrs),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !attrs_seq
            .into_iter()
            .all(|key| is_module(&key) || Task::is_attr(&key))
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Keys are not valid in {:?} must be attr or module",
                    self.proto_attrs
                ),
            ));
        }
        Ok(TaskValid::new(&self.proto_attrs.clone()))
    }
}
