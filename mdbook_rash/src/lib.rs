use rash_core::jinja::lookup::LOOKUPS;
use rash_core::modules::MODULES;

use std::fs;
use std::io;
use std::path::Path;
use std::sync::LazyLock;

use mdbook_core::book::{Book, BookItem, Chapter};
use mdbook_driver::builtin_preprocessors::LinkPreprocessor;
use mdbook_preprocessor::errors::Error;
use mdbook_preprocessor::{Preprocessor, PreprocessorContext};
use prettytable::{Table, format, row};
use regex::{Match, Regex};
use schemars::Schema;

#[macro_use]
extern crate log;

pub const SUPPORTED_RENDERER: &[&str] = &["markdown"];

const PUBLIC_SITE_URL: &str = "https://rash.sh";
const DOCS_RAW_BASE_URL: &str =
    "https://raw.githubusercontent.com/rash-sh/rash-sh.github.io/master/docs/rash";
const GITHUB_URL: &str = "https://github.com/rash-sh/rash";

static RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)                                                   # insignificant whitespace mode
        \{\s*                                                     # link opening parens and whitespace
        \$([a-zA-Z0-9_]+)                                         # link type
        (?:\s+                                                    # separating whitespace
        ([a-zA-Z0-9\s_.,\*\{\}\[\]\(\)\|'\-\\/`"\#+=:/\\%^?&<>;!@~\x{80}-\x{10FFFF}]+))?  # all doc
        \s*\}                                                     # whitespace and link closing parens"#
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
                    weight = new_section_number.first().unwrap() * 1000
                        + (ch.sub_items.len() + 1) as u32,
                    parameters = parameters,
                )
                .to_owned();

                let mut new_ch = Chapter::new(
                    name,
                    content_header,
                    format!("module_{}.md", name),
                    vec![ch.name.clone()],
                );
                new_ch.number = Some(new_section_number);
                info!("Add {} module", name);
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
                    weight = new_section_number.first().unwrap() * 1000
                        + (ch.sub_items.len() + 1) as u32,
                )
                .to_owned();

                let mut new_ch = Chapter::new(
                    lookup_name,
                    content_header,
                    format!("lookup_{}.md", lookup_name),
                    vec![ch.name.clone()],
                );
                new_ch.number = Some(new_section_number);
                info!("Add {} lookup", lookup_name);
                ch.sub_items.push(BookItem::Chapter(new_ch));
            }
            return ch.content = RE.replace(&ch.content, &indexes_body).to_string();
        };
        info!("Replace in chapter {}", ch.name);
        let other_content = &capture
            .1
            .clone()
            .unwrap_or_else(|| panic!("Empty include doc in {}.md", ch.name));
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

/// A page from the fully processed Markdown documentation output.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentationEntry {
    title: String,
    path: String,
    weight: u32,
    indent: bool,
}

fn parse_front_matter(content: &str) -> io::Result<Option<(String, u32, bool)>> {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return Ok(None);
    }

    let mut title = None;
    let mut weight = None;
    let mut indent = false;

    for line in lines {
        if line == "---" {
            break;
        }

        if let Some(value) = line.strip_prefix("title:") {
            title = Some(value.trim().trim_matches('"').to_owned());
        } else if let Some(value) = line.strip_prefix("weight:") {
            weight = Some(value.trim().parse::<u32>().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid documentation weight {value:?}: {error}"),
                )
            })?);
        } else if let Some(value) = line.strip_prefix("indent:") {
            indent = value.trim() == "true";
        }
    }

    match (title, weight) {
        (Some(title), Some(weight)) => Ok(Some((title, weight, indent))),
        _ => Ok(None),
    }
}

fn collect_markdown_entries(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<DocumentationEntry>,
) -> io::Result<()> {
    for item in fs::read_dir(directory)? {
        let item = item?;
        let path = item.path();

        if path.is_dir() {
            collect_markdown_entries(root, &path, entries)?;
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        let Some((title, weight, indent)) = parse_front_matter(&content)? else {
            continue;
        };
        let relative = path.strip_prefix(root).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("documentation path {} is outside {}: {error}", path.display(), root.display()),
            )
        })?;
        let relative = relative.to_string_lossy().replace('\\', "/");

        entries.push(DocumentationEntry {
            title,
            path: relative,
            weight,
            indent,
        });
    }

    Ok(())
}

fn render_llms_txt(entries: &[DocumentationEntry], docs_version: &str) -> String {
    let mut entries = entries.to_vec();
    entries.sort_by(|left, right| {
        left.weight
            .cmp(&right.weight)
            .then_with(|| left.path.cmp(&right.path))
    });

    let public_docs_url = format!("{PUBLIC_SITE_URL}/docs/rash/{docs_version}");
    let raw_docs_url = format!("{DOCS_RAW_BASE_URL}/{docs_version}");

    let mut output = String::from("# Rash\n\n");
    output.push_str(
        "> Rash is a declarative local automation tool using Ansible-like YAML tasks and ",
    );
    output.push_str(
        "MiniJinja templates, distributed as a single Rust binary with no runtime dependencies.\n\n",
    );

    if docs_version == "latest" {
        output.push_str(
            "This index covers the unreleased documentation built from Rash's `master` branch. ",
        );
        output.push_str(
            "For a released Rash version, use the `llms.txt` file under that version's documentation path.\n\n",
        );
    } else {
        output.push_str(&format!(
            "This index covers Rash documentation version `{docs_version}`. Keep answers within this version unless the user explicitly asks about another release.\n\n"
        ));
    }

    output.push_str(
        "For exact built-in module parameters, required fields, types, enum values and defaults, ",
    );
    output.push_str(
        "prefer the generated module reference, which is backed by Rash's JSON schemas when available ",
    );
    output.push_str(
        "and by the module's Rust documentation otherwise. Prefer generated lookup pages for lookup ",
    );
    output.push_str(
        "behavior, the CLI reference for command-line behavior, and the versioned book for language semantics.\n\n",
    );

    output.push_str("## Documentation\n\n");
    for entry in &entries {
        let prefix = if entry.indent { "  - " } else { "- " };
        output.push_str(&format!(
            "{prefix}[{}]({raw_docs_url}/{})\n",
            entry.title, entry.path
        ));
    }

    output.push_str("\n## Primary sources\n\n");
    output.push_str(&format!(
        "- [Human documentation]({public_docs_url}/): Rendered documentation for this version.\n"
    ));
    output.push_str(&format!(
        "- [Rash repository]({GITHUB_URL}): Runtime implementation, tests, examples, and documentation sources.\n"
    ));

    output
}

/// Generate a versioned llms.txt index from mdBook's fully processed Markdown output.
///
/// The input directory must be the output of Rash's Markdown renderer. This deliberately indexes
/// generated module and lookup pages as well as authored chapters, so the LLM navigation follows
/// the exact same documentation graph that is published for humans.
pub fn generate_llms_txt(docs_dir: &Path, docs_version: &str) -> io::Result<String> {
    let mut entries = Vec::new();
    collect_markdown_entries(docs_dir, docs_dir, &mut entries)?;
    Ok(render_llms_txt(&entries, docs_version))
}

#[cfg(test)]
mod llms_txt_test {
    use super::*;

    fn entries() -> Vec<DocumentationEntry> {
        vec![
            DocumentationEntry {
                title: "Modules".to_owned(),
                path: "modules.md".to_owned(),
                weight: 5000,
                indent: false,
            },
            DocumentationEntry {
                title: "file".to_owned(),
                path: "module_file.md".to_owned(),
                weight: 5001,
                indent: true,
            },
            DocumentationEntry {
                title: "Introduction".to_owned(),
                path: "index.md".to_owned(),
                weight: 0,
                indent: false,
            },
        ]
    }

    #[test]
    fn test_parse_front_matter() {
        let metadata = parse_front_matter(
            "---\ntitle: file\nweight: 5001\nindent: true\n---\n\n# file\n",
        )
        .unwrap();

        assert_eq!(metadata, Some(("file".to_owned(), 5001, true)));
    }

    #[test]
    fn test_render_llms_txt_uses_processed_markdown_links() {
        let output = render_llms_txt(&entries(), "v3.0");

        assert!(output.contains(
            "https://raw.githubusercontent.com/rash-sh/rash-sh.github.io/master/docs/rash/v3.0/index.md"
        ));
        assert!(output.contains(
            "  - [file](https://raw.githubusercontent.com/rash-sh/rash-sh.github.io/master/docs/rash/v3.0/module_file.md)"
        ));
        assert!(!output.contains("/docs/rash/latest/"));
    }

    #[test]
    fn test_latest_is_explicitly_unreleased() {
        let output = render_llms_txt(&entries(), "latest");

        assert!(output.contains("unreleased documentation"));
        assert!(output.contains("/docs/rash/latest/index.md"));
    }

    #[test]
    fn test_llms_txt_documents_source_precedence() {
        let output = render_llms_txt(&entries(), "v3.0");

        assert!(output.contains("JSON schemas"));
        assert!(output.contains("generated lookup pages"));
        assert!(output.contains("CLI reference"));
    }
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
mod weight_overflow_test {
    use super::*;

    struct Section {
        name: &'static str,
        base_weight: u32,
        next_section_weight: u32,
        item_count: usize,
    }

    fn get_sections() -> Vec<Section> {
        let module_count = MODULES.len();
        let lookup_count = LOOKUPS.len();
        vec![
            Section {
                name: "Modules",
                base_weight: 5000,
                next_section_weight: 6000,
                item_count: module_count,
            },
            Section {
                name: "Lookups",
                base_weight: 8000,
                next_section_weight: 9000,
                item_count: lookup_count,
            },
        ]
    }

    #[test]
    fn test_module_weights_do_not_overflow_into_next_section() {
        for section in get_sections() {
            for i in 1..=section.item_count {
                let weight = section.base_weight + i as u32;
                assert!(
                    weight < section.next_section_weight,
                    "{} item {} has weight {} which overflows into next section (weight {})",
                    section.name,
                    i,
                    weight,
                    section.next_section_weight,
                );
            }
        }
    }

    #[test]
    fn test_weight_formula_matches_base_plus_index() {
        let sections = get_sections();
        for section in &sections {
            let section_first = section.base_weight / 1000;
            for i in 1..=section.item_count {
                let calculated = section_first * 1000 + i as u32;
                assert_eq!(
                    calculated,
                    section.base_weight + i as u32,
                    "Weight formula changed for {} item {}",
                    section.name,
                    i,
                );
            }
        }
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
