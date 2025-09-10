use rash_core::jinja::lookup::LOOKUPS;
use rash_core::modules::MODULES;

use std::sync::LazyLock;

use mdbook::book::{Book, BookItem, Chapter};
use mdbook::errors::Error;
use mdbook::preprocess::{LinkPreprocessor, Preprocessor, PreprocessorContext};
use prettytable::{Table, format, row};
use regex::{Match, Regex};
use schemars::Schema;

#[macro_use]
extern crate log;

pub const SUPPORTED_RENDERER: &[&str] = &["markdown"];

static RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)                                                # insignificant whitespace mode
        \{\s*                                                  # link opening parens and whitespace
        \$([a-zA-Z0-9_]+)                                      # link type
        (?:\s+                                                 # separating whitespace
        ([a-zA-Z0-9\s_.,\*\{\}\[\]\(\)\|'\-\\/`"\#+=:/\\]+))?  # all doc
        \s*\}                                                  # whitespace and link closing parens"#
    )
    .unwrap()
});

static FORMAT: LazyLock<format::TableFormat> = LazyLock::new(|| {
    format::FormatBuilder::new()
        .padding(1, 1)
        .borders('|')
        .separator(
            format::LinePosition::Title,
            format::LineSeparator::new('-', '|', '|', '|'),
        )
        .column_separator('|')
        .build()
});

fn get_matches(ch: &Chapter) -> Option<Vec<(Match<'_>, Option<String>, String)>> {
    RE.captures_iter(&ch.content)
        .map(|cap| match (cap.get(0), cap.get(1), cap.get(2)) {
            (Some(origin), Some(typ), rest) => match (typ.as_str(), rest) {
                ("include_doc", Some(content)) => Some((
                    origin,
                    Some(content.as_str().replace("/// ", "").replace("///", "")),
                    typ.as_str().to_owned(),
                )),
                ("include_module_index" | "include_doc" | "include_lookup_index", _) => {
                    Some((origin, None, typ.as_str().to_owned()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Option<Vec<(Match, Option<String>, String)>>>()
}

fn get_type(val: &serde_json::Value) -> String {
    match val.get("type") {
        Some(t) => {
            if t.is_array() {
                t.as_array()
                    .unwrap()
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                t.as_str().unwrap_or("").to_string()
            }
        }
        None => "".to_string(),
    }
}

fn get_enum(val: &serde_json::Value) -> String {
    val.get("enum")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
                .join("<br>")
        })
        .unwrap_or_default()
}

fn get_description(val: &serde_json::Value) -> String {
    use regex::Regex;
    let re = Regex::new(r"\s+").unwrap();
    val.get("description")
        .and_then(|d| d.as_str())
        .map(|s| {
            // Replace all whitespace sequences (including newlines) with single spaces
            // and remove any remaining line breaks or carriage returns
            re.replace_all(s, " ")
                .replace(['\n', '\r'], " ")
                .replace("  ", " ")
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

fn format_schema(schema: &Schema) -> String {
    let mut table = Table::new();
    table.set_format(*FORMAT);
    table.set_titles(row![
        "Parameter",
        "Required",
        "Type",
        "Values",
        "Description"
    ]);

    let root = schema;
    let properties = root.get("properties").and_then(|p| p.as_object());
    let required = root
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let add_properties = |table: &mut Table, props: &serde_json::Map<String, serde_json::Value>| {
        for (name, prop_schema) in props {
            let value = get_enum(prop_schema);
            let description = get_description(prop_schema);

            table.add_row(row![
                name,
                if required.contains(name) {
                    "true".to_string()
                } else {
                    "".to_string()
                },
                get_type(prop_schema),
                value,
                description
            ]);
        }
    };

    if let Some(one_of) = root.get("oneOf").and_then(|o| o.as_array()) {
        for schema in one_of {
            let variant_description = get_description(schema);
            if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                // For oneOf variants, we need to apply the variant's description to its properties
                for (name, prop_schema) in props {
                    let value = get_enum(prop_schema);
                    // Use the variant's description if the property doesn't have one
                    let mut prop_description = get_description(prop_schema);
                    if prop_description.is_empty() && !variant_description.is_empty() {
                        prop_description = variant_description.clone();
                    }

                    table.add_row(row![
                        name,
                        if required.contains(name) {
                            "true".to_string()
                        } else {
                            "".to_string()
                        },
                        get_type(prop_schema),
                        value,
                        prop_description
                    ]);
                }
            }
        }
    }

    if let Some(props) = properties {
        add_properties(&mut table, props);
    }

    format!("{table}")
}

fn replace_matches(captures: Vec<(Match, Option<String>, String)>, ch: &mut Chapter) {
    for capture in captures.iter() {
        if capture.2 == "include_module_index" {
            let mut indexes_vec = MODULES
                .keys()
                .map(|name| format!("- [{name}](./module_{name}.html)"))
                .collect::<Vec<String>>();
            indexes_vec.sort();
            let indexes_body = indexes_vec.join("\n");

            let mut modules = MODULES.iter().collect::<Vec<_>>();
            modules.sort_by_key(|x| x.0);

            for module in modules {
                let mut new_section_number = ch.number.clone().unwrap();
                new_section_number.push((ch.sub_items.len() + 1) as u32);

                let schema = module.1.get_json_schema();
                let name = module.0;

                let parameters = schema.map(|s| format_schema(&s)).unwrap_or_else(|| format!("{{$include_doc {{{{#include ../../rash_core/src/modules/{name}.rs:parameters}}}}}}"));
                let content_header = format!(
                    r#"---
title: {name}
weight: {weight}
indent: true
---

{{$include_doc {{{{#include ../../rash_core/src/modules/{name}.rs:module}}}}}}

## Parameters

{parameters}
{{$include_doc {{{{#include ../../rash_core/src/modules/{name}.rs:examples}}}}}}

"#,
                    name = name,
                    weight = (new_section_number.clone().first().unwrap() + 1) * 1000
                        + ((ch.sub_items.len() + 1) * 100) as u32,
                    parameters = parameters,
                )
                .to_owned();

                let mut new_ch = Chapter::new(
                    name,
                    content_header,
                    format!("module_{}.md", &name),
                    vec![ch.name.clone()],
                );
                new_ch.number = Some(new_section_number);
                info!("Add {} module", &name);
                ch.sub_items.push(BookItem::Chapter(new_ch));
            }
            return ch.content = RE.replace(&ch.content, &indexes_body).to_string();
        } else if capture.2 == "include_lookup_index" {
            let mut indexes_vec = LOOKUPS
                .iter()
                .map(|name| format!("- [{name}](./lookup_{name}.html)"))
                .collect::<Vec<String>>();
            indexes_vec.sort();
            let indexes_body = indexes_vec.join("\n");

            let mut lookups = LOOKUPS.iter().collect::<Vec<_>>();
            lookups.sort();

            for lookup_name in lookups {
                let mut new_section_number = ch.number.clone().unwrap();
                new_section_number.push((ch.sub_items.len() + 1) as u32);

                let content_header = format!(
                    r#"---
title: {name}
weight: {weight}
indent: true
---

{{$include_doc {{{{#include ../../rash_core/src/jinja/lookup/{name}.rs:lookup}}}}}}

{{$include_doc {{{{#include ../../rash_core/src/jinja/lookup/{name}.rs:examples}}}}}}

"#,
                    name = lookup_name,
                    weight = (new_section_number.clone().first().unwrap() + 1) * 1000
                        + ((ch.sub_items.len() + 1) * 100) as u32,
                )
                .to_owned();

                let mut new_ch = Chapter::new(
                    lookup_name,
                    content_header,
                    format!("lookup_{}.md", &lookup_name),
                    vec![ch.name.clone()],
                );
                new_ch.number = Some(new_section_number);
                info!("Add {} lookup", &lookup_name);
                ch.sub_items.push(BookItem::Chapter(new_ch));
            }
            return ch.content = RE.replace(&ch.content, &indexes_body).to_string();
        };
        info!("Replace in chapter {}", &ch.name);
        let other_content = &capture
            .1
            .clone()
            .unwrap_or_else(|| panic!("Empty include doc in {}.md", &ch.name));
        ch.content = RE.replace(&ch.content, other_content).to_string();
    }
}

fn escape_jekyll(ch: &mut Chapter) {
    let mut new_content = ch.content.replace("\n---\n", "\n---\n\n{% raw %}");
    new_content.push_str("{% endraw %}");
    ch.content = new_content;
}

fn preprocess_rash(book: &mut Book, is_escape_jekyll: bool) {
    book.for_each_mut(|section: &mut BookItem| {
        if let BookItem::Chapter(ref mut ch) = *section {
            let ch_copy = ch.clone();
            if let Some(captures) = get_matches(&ch_copy) {
                replace_matches(captures, ch);
            };
            if is_escape_jekyll {
                escape_jekyll(ch);
            };
        };
    });
}

pub fn run(_ctx: &PreprocessorContext, book: Book) -> Result<Book, Error> {
    let mut new_book = book;
    preprocess_rash(&mut new_book, false);

    let mut processed_book = LinkPreprocessor::new().run(_ctx, new_book.clone())?;

    preprocess_rash(&mut processed_book, true);
    Ok(processed_book)
}

#[cfg(test)]
mod prettytable_wrap_test {
    use prettytable::{Table, row};

    #[test]
    fn test_long_description_row() {
        let mut table = Table::new();
        table.set_titles(row![
            "Parameter",
            "Required",
            "Type",
            "Values",
            "Description"
        ]);
        table.add_row(row![
            "argv",
            "",
            "array",
            "",
            "Passes the command arguments as a list rather than a string. Only the string or the list form can be provided, not both."
        ]);
        table.add_row(row![
            "transfer_pid",
            "",
            "boolean",
            "",
            "Execute command as PID 1. Note: from this point on, your rash script execution is transferred to the command."
        ]);
        println!("{table}");
    }
}

#[cfg(test)]
mod schema_debug_test {
    use super::*;

    #[test]
    fn debug_command_schema() {
        if let Some(command_module) = MODULES.get("command")
            && let Some(schema) = command_module.get_json_schema()
        {
            println!("=== COMMAND SCHEMA ===");
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
            println!("=== END COMMAND SCHEMA ===");

            let table_output = format_schema(&schema);
            println!("=== TABLE OUTPUT ===");
            println!("{table_output}");
            println!("=== END TABLE OUTPUT ===");
        }
    }
}
