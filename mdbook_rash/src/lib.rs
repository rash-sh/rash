use mdbook::book::{Book, BookItem, Chapter};
use mdbook::errors::Error;
use mdbook::preprocess::{LinkPreprocessor, Preprocessor, PreprocessorContext};
use regex::{Match, Regex};

use rash_core::modules::MODULES;

#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

pub static SUPPORTED_RENDERER: &[&str] = &["html", "markdown"];

lazy_static! {
    static ref RE: Regex = Regex::new(
        r#"(?x)                                          # insignificant whitespace mode
        \{\s*                                            # link opening parens and whitespace
        \#([a-zA-Z0-9_]+)                                # link type
        (?:\s+                                           # separating whitespace
        ([a-zA-Z0-9\s_.,\[\]\(\)\|'\-\\/`"\#+=:/\\]+))?  # all doc
        \s*\}                                            # whitespace and link closing parens"#
    )
    .unwrap();
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

fn replace_matches(captures: Vec<(Match, Option<String>, String)>, ch: &mut Chapter) {
    for capture in captures.iter() {
        if capture.2 == "include_module_index" {
            let indexes_body = MODULES
                .clone()
                .into_iter()
                .map(|(name, _)| format!("- [{}](./{}.html)", &name, &name))
                .collect::<Vec<String>>()
                .join("\n");

            for module in MODULES.clone().into_iter() {
                let mut new_section_number = ch.number.clone().unwrap();
                new_section_number.push((ch.sub_items.len() + 1) as u32);

                let name = module.0;
                let content_header = format!(
                    r#"---
title: {}
weight: {}
indent: true
---

{{#include_doc {{{{#include ../../rash_core/src/modules/{}.rs:module}}}}}}
"#,
                    name,
                    (new_section_number.clone().first().unwrap() + 1) * 1000
                        + ((ch.sub_items.len() + 1) * 100) as u32,
                    name
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
