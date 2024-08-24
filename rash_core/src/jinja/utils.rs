use std::collections::BTreeMap;

use minijinja::{context, value::ValueKind, Value};

#[inline(always)]
pub fn extend_vars(a: Value, b: Value) -> Value {
    context! {
    ..a,
    ..b
    }
}

pub fn merge(a: Value, b: Value) -> Value {
    match (&a.kind(), &b.kind()) {
        (ValueKind::Map, ValueKind::Map) => {
            let mut merged_map = BTreeMap::new();

            for key in a
                .try_iter()
                .unwrap()
                .map(|x| x.as_str().unwrap().to_string())
            {
                let a_value = a.get_attr(&key).unwrap();
                let b_value = b.get_attr(&key).unwrap_or(Value::UNDEFINED);

                let merged_value = if b_value.is_undefined() {
                    a_value
                } else {
                    merge(a_value, b_value)
                };

                merged_map.insert(key, merged_value);
            }

            for key in b
                .try_iter()
                .unwrap()
                .map(|x| x.as_str().unwrap().to_string())
            {
                if !merged_map.contains_key(&key) {
                    merged_map.insert(key.clone(), b.get_attr(&key).unwrap());
                }
            }

            Value::from(merged_map)
        }

        (ValueKind::Seq, ValueKind::Seq) => {
            let mut combined_seq = b.try_iter().unwrap().collect::<Vec<Value>>();
            combined_seq.extend(a.try_iter().unwrap());
            Value::from(combined_seq)
        }
        (ValueKind::Number, ValueKind::Number) => {
            Value::from(a.as_i64().unwrap() + b.as_i64().unwrap())
        }
        (_, ValueKind::Undefined) => a,
        (ValueKind::Undefined, _) => b,
        _ => {
            if a.kind() != ValueKind::Map {
                return a;
            };
            if b.kind() != ValueKind::Map {
                return a;
            };
            extend_vars(a, b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extend_vars() {
        let a = context! { a => "foo"};
        let b = context! { b => "boo"};
        let ctx = extend_vars(a, b);

        assert_eq!(ctx.get_attr("a").unwrap(), Value::from("foo"));
        assert_eq!(ctx.get_attr("b").unwrap(), Value::from("boo"));

        let a = context! { a => context!{ b => "foo"}};
        let b = context! { a => context!{ c => "boo"}};
        let ctx = extend_vars(a, b);

        assert_eq!(ctx.get_attr("a").unwrap(), context! { b => "foo"});
    }

    #[test]
    fn test_merge() {
        let a = context! { a => context!{ b => "foo"}};
        let b = context! { a => context!{ c => "boo"}};
        let ctx = merge(a, b);

        assert_eq!(
            ctx.get_attr("a").unwrap(),
            context! { b => "foo", c => "boo"}
        );
    }

    #[test]
    fn test_merge_overlapping() {
        let a = context! { a => "foo" };
        let b = context! { a => "boo" };
        let ctx = merge(a, b);
        assert_eq!(ctx.get_attr("a").unwrap(), Value::from("foo"));
    }

    #[test]
    fn test_merge_overlapping_nested() {
        let a = context! { a => context! { b => "foo" } };
        let b = context! { a => context! { c => "boo" } };
        let ctx = merge(a, b);
        assert_eq!(
            ctx.get_attr("a").unwrap(),
            context! { b => "foo", c => "boo" }
        );
    }

    #[test]
    fn test_merge_mixed_types() {
        let a = context! { a => context! { b => "foo" } };
        let b = context! { a => "simple_value" };
        let ctx = merge(a, b);
        assert_eq!(ctx.get_attr("a").unwrap(), context! { b => "foo" });
    }

    #[test]
    fn test_merge_with_empty() {
        let a = context! {};
        let b = context! { a => "foo" };
        let ctx = merge(a, b);
        assert_eq!(ctx.get_attr("a").unwrap(), Value::from("foo"));

        let a = context! { a => "foo" };
        let b = context! {};
        let ctx = merge(a, b);
        assert_eq!(ctx.get_attr("a").unwrap(), Value::from("foo"));
    }

    #[test]
    fn test_merge_deeply_nested() {
        let a = context! { a => context! { b => context! { c => "foo" } } };
        let b = context! { a => context! { b => context! { d => "boo" } } };
        let ctx = merge(a, b);
        assert_eq!(
            ctx.get_attr("a").unwrap(),
            context! { b => context! { c => "foo", d => "boo" } }
        );
    }

    #[test]
    fn test_merge_deeply_nested_partially_overlap() {
        let a = context! { a => context! { b => context! { c => "foo", e => "hello" } } };
        let b = context! { a => context! { b => context! { d => "boo", e => "world" } } };
        let ctx = merge(a, b);
        assert_eq!(
            ctx.get_attr("a").unwrap(),
            context! { b => context! { c => "foo", d => "boo", e => "hello" } }
        );
    }

    #[test]
    fn test_merge_add_top_level() {
        let a = context! { a => "foo" };
        let b = context! { b => "boo" };
        let ctx = merge(a, b);
        assert_eq!(ctx.get_attr("a").unwrap(), Value::from("foo"));
        assert_eq!(ctx.get_attr("b").unwrap(), Value::from("boo"));
    }

    #[test]
    fn test_merge_both_empty() {
        let a = context! {};
        let b = context! {};
        let ctx = merge(a, b);
        assert_eq!(ctx, context! {});
    }

    #[test]
    fn test_merge_seq_concatenation() {
        let a = context! { a => vec![4, 5, 6] };
        let b = context! { a => vec![1, 2, 3] };
        let ctx = merge(a, b);
        assert_eq!(
            ctx.get_attr("a").unwrap(),
            Value::from(vec![1, 2, 3, 4, 5, 6])
        );
    }

    #[test]
    fn test_merge_seq_with_non_seq() {
        let a = context! { a => vec![1, 2, 3] };
        let b = context! { a => "override" };
        let ctx = merge(a, b);
        assert_eq!(ctx.get_attr("a").unwrap(), Value::from(vec![1, 2, 3]));
    }

    #[test]
    fn test_merge_nested_seq_concatenation() {
        let a = context! { a => context! { b => vec![3, 4] } };
        let b = context! { a => context! { b => vec![1, 2] } };
        let ctx = merge(a, b);
        assert_eq!(
            ctx.get_attr("a").unwrap().get_attr("b").unwrap(),
            Value::from(vec![1, 2, 3, 4])
        );
    }

    #[test]
    fn test_merge_deeply_nested_mixed_with_seq() {
        let a = context! { a => context! { b => context! { c => vec![3, 4], e => "hello" } } };
        let b = context! { a => context! { b => context! { c => vec![1, 2], e => "world" } } };
        let ctx = merge(a, b);
        assert_eq!(
            ctx.get_attr("a").unwrap(),
            context! { b => context! { c => vec![1, 2, 3, 4], e => "hello" } }
        );
    }
}
