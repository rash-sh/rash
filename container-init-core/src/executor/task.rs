use yaml_rust::Yaml;

#[derive(Debug)]
pub struct Task {}

impl Task {
    pub fn from(_task_yaml: &Yaml) -> Self {
        Task {}
    }
}
