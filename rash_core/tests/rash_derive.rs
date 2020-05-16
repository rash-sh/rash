#[cfg(test)]
mod tests {
    use rash_derive::FieldNames;
    use std::collections::HashSet;

    #[allow(dead_code)]
    #[derive(FieldNames)]
    struct Test {
        foo: bool,
        boo: u8,
    }

    #[test]
    fn test_fieldnames() {
        assert_eq![
            Test::get_field_names(),
            ["foo", "boo"]
                .iter()
                .map(ToString::to_string)
                .collect::<HashSet<String>>()
        ];
    }
}
