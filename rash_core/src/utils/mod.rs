pub mod tera;

use crate::error::{Error, ErrorKind, Result};

use serde_yaml::Value;
use yaml_rust::{Yaml, YamlEmitter, YamlLoader};

pub fn parse_octal(s: &str) -> Result<u32> {
    match s.len() {
        3 => u32::from_str_radix(s, 8).map_err(|e| Error::new(ErrorKind::InvalidData, e)),
        4 => u32::from_str_radix(s.get(1..).unwrap(), 8)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e)),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{} cannot be parsed to octal", s),
        )),
    }
}

pub fn get_yaml(s: &str) -> Result<Yaml> {
    let doc = YamlLoader::load_from_str(s).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    Ok(doc.first().unwrap().clone())
}

pub fn get_string(yaml: Yaml) -> Result<String> {
    let mut yaml_str = String::new();
    let mut emitter = YamlEmitter::new(&mut yaml_str);
    emitter
        .dump(&yaml)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    Ok(yaml_str)
}

pub fn get_serde_yaml(yaml: &Yaml) -> Result<Value> {
    let mut yaml_str = String::new();
    let mut emitter = YamlEmitter::new(&mut yaml_str);
    emitter
        .dump(yaml)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    serde_yaml::from_str(&yaml_str).map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

pub fn merge_json(a: &mut serde_json::Value, b: serde_json::Value) {
    match (a, b) {
        (a @ &mut serde_json::Value::Object(_), serde_json::Value::Object(b)) => {
            let a = a.as_object_mut().unwrap();
            for (k, v) in b {
                if v.is_array() && a.contains_key(&k) && a.get(&k).as_ref().unwrap().is_array() {
                    let mut _a = a.get(&k).unwrap().as_array().unwrap().to_owned();
                    _a.append(&mut v.as_array().unwrap().to_owned());
                    a[&k] = serde_json::Value::from(_a);
                } else if v.is_u64() && a.contains_key(&k) && a.get(&k).as_ref().unwrap().is_u64() {
                    let _a = a.get(&k).unwrap().as_u64().unwrap().to_owned();
                    let _v = v.as_u64().unwrap().to_owned();
                    a[&k] = serde_json::Value::from(_a + _v);
                } else {
                    merge_json(a.entry(k).or_insert(serde_json::Value::Null), v);
                }
            }
        }
        (a, b) => *a = b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_octal() {
        assert_eq!(parse_octal("644").unwrap(), 0o644);
        assert_eq!(parse_octal("0644").unwrap(), 0o644);
        assert_eq!(parse_octal("777").unwrap(), 0o777);
        assert_eq!(parse_octal("0444").unwrap(), 0o444);
        assert_eq!(parse_octal("600").unwrap(), 0o600);
        assert_eq!(parse_octal("0600").unwrap(), 0o600);
    }

    #[test]
    fn test_get_yaml() {
        let yaml = get_yaml("foo: boo").unwrap();
        assert_eq!(yaml["foo"].as_str().unwrap(), "boo")
    }

    #[test]
    fn test_get_string() {
        let yaml = YamlLoader::load_from_str(
            r#"
            path: /yea
            state: file
            mode: "0644"
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let yaml_string = get_string(yaml).unwrap();
        assert_eq!(
            yaml_string,
            r#"---
path: /yea
state: file
mode: "0644""#
                .to_string()
        )
    }
}
