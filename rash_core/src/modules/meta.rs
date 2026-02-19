use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::Schema;
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug)]
pub struct Meta;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaAction {
    FlushHandlers,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Params {
    action: MetaAction,
}

impl Module for Meta {
    fn get_name(&self) -> &str {
        "meta"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = serde_norway::from_value(params).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid meta parameters: {e}"),
            )
        })?;

        match params.action {
            MetaAction::FlushHandlers => {
                debug!("meta: flush_handlers triggered");
                let result = ModuleResult::new(
                    false,
                    Some(YamlValue::String("flush_handlers".to_string())),
                    None,
                );
                Ok((result, None))
            }
        }
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    fn create_test_global_params() -> GlobalParams<'static> {
        GlobalParams::default()
    }

    #[test]
    fn test_meta_module_get_name() {
        let meta = Meta;
        assert_eq!(meta.get_name(), "meta");
    }

    #[test]
    fn test_meta_flush_handlers() {
        let meta = Meta;
        let global_params = create_test_global_params();
        let params = YamlValue::Mapping(
            vec![(
                YamlValue::String("action".to_string()),
                YamlValue::String("flush_handlers".to_string()),
            )]
            .into_iter()
            .collect(),
        );
        let vars = context! {};

        let result = meta.exec(&global_params, params, &vars, false);
        assert!(result.is_ok());

        let (module_result, _value) = result.unwrap();
        assert!(!module_result.get_changed());
        assert_eq!(
            module_result.get_extra(),
            Some(YamlValue::String("flush_handlers".to_string()))
        );
    }
}
