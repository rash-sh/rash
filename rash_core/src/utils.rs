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
    if let (Some(a_map), Some(b_map)) = (a.as_object_mut(), b.as_object()) {
        for (k, v) in b_map {
            match (a_map.get_mut(k), &v) {
                (Some(serde_json::Value::Array(a_arr)), serde_json::Value::Array(b_arr)) => {
                    a_arr.extend(b_arr.clone());
                }
                (Some(serde_json::Value::Number(a_num)), serde_json::Value::Number(b_num))
                    if a_num.is_u64() && b_num.is_u64() =>
                {
                    let sum = a_num.as_u64().unwrap() + b_num.as_u64().unwrap();
                    a_map.insert(k.to_string(), serde_json::Value::from(sum));
                }
                (Some(a_val), _) => {
                    merge_json(a_val, v.clone());
                }
                (None, _) => {
                    a_map.insert(k.to_string(), v.clone());
                }
            }
        }
    } else {
        *a = b;
    }
}

pub fn default_false() -> Option<bool> {
    Some(false)
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
    fn test_merge() {
        let mut a = json!({ "a": { "b": "foo" } });
        let b = json!({ "a": { "c": "boo" } });
        merge_json(&mut a, b);

        assert_eq!(a.get("a").unwrap(), &json!({ "b": "foo", "c": "boo" }));
    }

    #[test]
    fn test_merge_overlapping() {
        let mut a = json!({ "a": "foo" });
        let b = json!({ "a": "boo" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("boo"));
    }

    #[test]
    fn test_merge_overlapping_nested() {
        let mut a = json!({ "a": { "c": "boo" } });
        let b = json!({ "a": { "b": "foo" } });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!({ "b": "foo", "c": "boo" }));
    }

    #[test]
    fn test_merge_mixed_types() {
        let mut a = json!({ "a": "simple_value" });
        let b = json!({ "a": { "b": "foo" } });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!({ "b": "foo" }));
    }

    #[test]
    fn test_merge_with_empty() {
        let mut a = json!({});
        let b = json!({ "a": "foo" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("foo"));

        let mut a = json!({ "a": "foo" });
        let b = json!({});
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("foo"));
    }

    #[test]
    fn test_merge_deeply_nested() {
        let mut a = json!({ "a": { "b": { "d": "boo" } } });
        let b = json!({ "a": { "b": { "c": "foo" } } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap(),
            &json!({ "b": { "c": "foo", "d": "boo" } })
        );
    }

    #[test]
    fn test_merge_deeply_nested_partially_overlap() {
        let mut a = json!({ "a": { "b": { "d": "boo", "e": "world" } } });
        let b = json!({ "a": { "b": { "c": "foo", "e": "hello" } } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap(),
            &json!({ "b": { "c": "foo", "d": "boo", "e": "hello" } })
        );
    }

    #[test]
    fn test_merge_add_top_level() {
        let mut a = json!({ "a": "foo" });
        let b = json!({ "b": "boo" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("foo"));
        assert_eq!(a.get("b").unwrap(), &json!("boo"));
    }

    #[test]
    fn test_merge_both_empty() {
        let mut a = json!({});
        let b = json!({});
        merge_json(&mut a, b);
        assert_eq!(a, json!({}));
    }

    #[test]
    fn test_merge_seq_concatenation() {
        let mut a = json!({ "a": vec![1, 2, 3] });
        let b = json!({ "a": vec![4, 5, 6] });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!(vec![1, 2, 3, 4, 5, 6]));
    }

    #[test]
    fn test_merge_seq_with_non_seq() {
        let mut a = json!({ "a": vec![1, 2, 3] });
        let b = json!({ "a": "override" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("override"));
    }

    #[test]
    fn test_merge_nested_seq_concatenation() {
        let mut a = json!({ "a": { "b": vec![1, 2] } });
        let b = json!({ "a": { "b": vec![3, 4] } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap().get("b").unwrap(),
            &json!(vec![1, 2, 3, 4])
        );
    }

    #[test]
    fn test_merge_deeply_nested_mixed_with_seq() {
        let mut a = json!({ "a": { "b": { "c": vec![1, 2], "e": "hello" } } });
        let b = json!({ "a": { "b": { "c": vec![3, 4], "e": "world" } } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap(),
            &json!({ "b": { "c": vec![1, 2, 3, 4], "e": "world" } })
        );
    }
}
