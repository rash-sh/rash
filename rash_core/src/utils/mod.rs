pub mod tera;

use crate::error::{Error, ErrorKind, Result};

pub fn parse_octal(s: &str) -> Result<u32> {
    match s.len() {
        3 => u32::from_str_radix(s, 8).map_err(|e| Error::new(ErrorKind::InvalidData, e)),
        4 => u32::from_str_radix(s.get(1..).unwrap(), 8)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e)),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{s} cannot be parsed to octal"),
        )),
    }
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
}
