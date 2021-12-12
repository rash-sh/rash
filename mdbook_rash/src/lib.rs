use mdbook::book::{Book, BookItem, Chapter};
use mdbook::errors::Error;
use mdbook::preprocess::PreprocessorContext;
use regex::{Captures, Match, Regex};

#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

pub static SUPPORTED_RENDERER: &[&str] = &["html", "markdown"];

lazy_static! {
    static ref RE: Regex = Regex::new(
        r#"(?x)                                       # insignificant whitespace mode
        \\\{\#.*\}                                    # match escaped link
        |                                             # or
        \{\s*                                         # link opening parens and whitespace
        \#([a-zA-Z0-9_]+)                             # link type
        \s+                                           # separating whitespace
        ([a-zA-Z0-9\s_.,\[\]\(\)\|'\-\\/`"\#+=:/\\]+) # all doc
        \s*\}                                         # whitespace and link closing parens"#
    )
    .unwrap();
}

fn get_matches(ch: &Chapter) -> Option<Vec<(Match, String, String)>> {
    RE.captures_iter(&ch.content)
        .map(|cap| match (cap.get(0), cap.get(1), cap.get(2)) {
            (Some(origin), Some(typ), Some(rest)) => match (typ.as_str(), rest.as_str()) {
                ("include_module", content) | ("include_doc", content) => Some((
                    origin,
                    content.replace("/// ", "").replace("///", ""),
                    typ.as_str().to_string(),
                )),
                _ => None,
            },
            _ => None,
        })
        .collect::<Option<Vec<(Match, String, String)>>>()
}

fn replace_matches(captures: Vec<(Match, String, String)>, ch: &mut Chapter) {
    for capture in captures.iter() {
        let name = capture
            .1
            .clone()
            .split('\n')
            .next()
            .unwrap()
            .replace("# ", "");
        let mut new_section_number = ch.number.clone().unwrap();
        new_section_number.push((ch.sub_items.len() + 1) as u32);

        let content_header = if capture.2 == "include_module" {
            format!(
                r#"---
title: {}
weight: {}
indent: true
---
"#,
                name,
                (new_section_number.clone().first().unwrap() + 1) * 1000
                    + ((ch.sub_items.len() + 1) * 100) as u32
            )
            .to_owned()
        } else {
            "".to_owned()
        };

        let new_content = content_header + &capture.1.clone();
        let mut new_ch = Chapter::new(
            &name,
            new_content.clone(),
            format!("{}.md", &name),
            vec![ch.name.clone()],
        );

        new_ch.number = Some(new_section_number);
        if capture.2 == "include_module" {
            ch.sub_items.push(BookItem::Chapter(new_ch));
        }
        info!("Add {}", &name);
        ch.content = RE
            .replace(&ch.content, |caps: &Captures| match &caps[1] {
                "include_module" => format!("- [{}](./{}.html)", &name, &name),
                _ => new_content.clone(),
            })
            .to_string();
    }
}

fn escape_jekyll(ch: &mut Chapter) {
    let mut new_content = ch.content.replace("\n---\n", "\n---\n\n{% raw %}");
    new_content.push_str("{% endraw %}");
    ch.content = new_content;
}

pub fn run(_ctx: &PreprocessorContext, book: Book) -> Result<Book, Error> {
    let mut new_book = book;

    new_book.for_each_mut(|section: &mut BookItem| {
        if let BookItem::Chapter(ref mut ch) = *section {
            let ch_copy = ch.clone();
            if let Some(captures) = get_matches(&ch_copy) {
                replace_matches(captures, ch);
            };
            escape_jekyll(ch)
        }
    });

    Ok(new_book)
}
