use rash_core::modules::MODULES;

use mdbook::book::{Book, BookItem, Chapter};
use mdbook::errors::Error;
use mdbook::preprocess::{LinkPreprocessor, Preprocessor, PreprocessorContext};
use prettytable::{cell, format, row, Table};
use regex::{Match, Regex};
use schemars::schema::{RootSchema, SingleOrVec};

#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

pub static SUPPORTED_RENDERER: &[&str] = &["markdown"];

lazy_static! {
    static ref RE: Regex = Regex::new(
        r#"(?x)                                                # insignificant whitespace mode
        \{\s*                                                  # link opening parens and whitespace
        \$([a-zA-Z0-9_]+)                                      # link type
        (?:\s+                                                 # separating whitespace
        ([a-zA-Z0-9\s_.,\*\{\}\[\]\(\)\|'\-\\/`"\#+=:/\\]+))?  # all doc
        \s*\}                                                  # whitespace and link closing parens"#
    )
    .unwrap();
    static ref FORMAT: format::TableFormat = format::FormatBuilder::new()
        .padding(1, 1)
        .borders('|')
        .separator(
            format::LinePosition::Title,
            format::LineSeparator::new('-', '|', '|', '|'),
        )
        .column_separator('|')
        .build();
}

fn get_matches(ch: &Chapter) -> Option<Vec<(Match, Option<String>, String)>> {
    RE.captures_iter(&ch.content)
        .map(|cap| match (cap.get(0), cap.get(1), cap.get(2)) {
            (Some(origin), Some(typ), rest) => match (typ.as_str(), rest) {
                ("include_doc", Some(content)) => Some((
                    origin,
                    Some(content.as_str().replace("/// ", "").replace("///", "")),
                    typ.as_str().to_string(),
                )),
                ("include_module_index" | "include_doc", _) => {
                    Some((origin, None, typ.as_str().to_string()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Option<Vec<(Match, Option<String>, String)>>>()
}

fn format_schema(schema: &RootSchema) -> String {
    let mut table = Table::new();

    // safe unwrap: all Params are objects
    let object_validation = schema.clone().schema.object.unwrap();
    let required_values = object_validation.required;
    table.set_format(*FORMAT);

    table.set_titles(row![
        "Parameter",
        "Required",
        "Type",
        "Values",
        "Description"
    ]);

    if let Some(subschema) = schema.clone().schema.subschemas {
        if let Some(schema_objects) = subschema.one_of {
            for schema in schema_objects {
                let schema_object = schema.into_object();
                let metadata = schema_object.metadata;
                let description = match metadata {
                    Some(x) => match x.description {
                        Some(d) => d,
                        None => "".to_string(),
                    },
                    None => "".to_string(),
                };
                for property in schema_object.object.unwrap().properties {
                    let name = property.0;
                    let schema_object = property.1.into_object();
                    let value = match schema_object.enum_values {
                        Some(x) => x
                            .into_iter()
                            .map(|i| {
                                serde_yaml::to_value(i)
                                    .unwrap()
                                    .as_str()
                                    .unwrap()
                                    .to_owned()
                            })
                            .collect::<Vec<String>>()
                            .join("<br>"),
                        None => "".to_string(),
                    };
                    table.add_row(row![
                        name,
                        match required_values.contains(&name) {
                            true => "true",
                            false => "",
                        },
                        match schema_object.instance_type {
                            Some(s) => {
                                let t = match s {
                                    SingleOrVec::Vec(v) => v[0],
                                    SingleOrVec::Single(x) => *x,
                                };
                                serde_yaml::to_value(&t)
                                    .unwrap()
                                    .as_str()
                                    .unwrap()
                                    .to_owned()
                            }
                            None => "".to_string(),
                        },
                        value,
                        description
                    ]);
                }
            }
        }
    };

    for property in object_validation.properties.into_iter() {
        let name = property.0;
        let schema_object = property.1.into_object();
        let metadata = schema_object.metadata;
        let description = match metadata {
            Some(x) => match x.description {
                Some(d) => d,
                None => "".to_string(),
            },
            None => "".to_string(),
        };

        let value = match schema_object.enum_values {
            Some(x) => x
                .into_iter()
                .map(|i| {
                    serde_yaml::to_value(i)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .collect::<Vec<String>>()
                .join("<br>"),
            None => "".to_string(),
        };
        table.add_row(row![
            name,
            match required_values.contains(&name) {
                true => "true",
                false => "",
            },
            match schema_object.instance_type {
                Some(s) => {
                    let t = match s {
                        SingleOrVec::Vec(v) => v[0],
                        SingleOrVec::Single(x) => *x,
                    };
                    serde_yaml::to_value(&t)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                }
                None => "".to_string(),
            },
            value,
            description
        ]);
    }
    format!("{}", table)
}

fn replace_matches(captures: Vec<(Match, Option<String>, String)>, ch: &mut Chapter) {
    for capture in captures.iter() {
        if capture.2 == "include_module_index" {
            let mut indexes_vec = MODULES
                .iter()
                .map(|(name, _)| format!("- [{}](./{}.html)", &name, &name))
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

                let parameters = match schema {
                    Some(s) => format_schema(&s),
                    None => format!("{{$include_doc {{{{#include ../../rash_core/src/modules/{}.rs:parameters}}}}}}", name)
                };

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
                    format!("{}.md", &name),
                    vec![ch.name.clone()],
                );
                new_ch.number = Some(new_section_number);
                info!("Add {} module", &name);
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
