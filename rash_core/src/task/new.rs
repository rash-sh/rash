use crate::error::{Error, ErrorKind, Result};
use crate::modules::is_module;
use crate::task::valid::TaskValid;
use crate::task::Task;

use yaml_rust::Yaml;

/// TaskNew is a new task without Yaml verified
#[derive(Debug)]
pub struct TaskNew {
    proto_attrs: Yaml,
}

impl From<&Yaml> for TaskNew {
    fn from(yaml: &Yaml) -> Self {
        TaskNew {
            proto_attrs: yaml.clone(),
        }
    }
}

impl TaskNew {
    /// Validate all `proto_attrs` can be represented as String and are task fields or modules
    pub fn validate_attrs(&self) -> Result<TaskValid> {
        let attrs_hash = self.proto_attrs.clone().into_hash().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Task is not a dict {:?}", self.proto_attrs),
            )
        })?;
        let attrs_vec = attrs_hash
            .iter()
            .map(|(key, _)| {
                key.as_str().ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Key is not valid in {:?}", self.proto_attrs),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !attrs_vec
            .into_iter()
            .all(|key| is_module(key) || Task::is_attr(key))
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
