use rash_core::jinja::lookup::LOOKUPS;
use rash_core::modules::MODULES;

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

const DOCS_BASE_URL: &str = "https://rash-sh.github.io/docs/rash/latest";
const GITHUB_URL: &str = "https://github.com/rash-sh/rash";

static RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)                                                   # insignificant whitespace mode
        \{\s*                                                     # link opening parens and whitespace
        \$([a-zA-Z0-9_]+)                                         # link type
        (?:\s+                                                    # separating whitespace
        ([a-zA-Z0-9\s_.,\*\{\}\[\]\(\)\|'\-\\/`"\#+=:/\\%^?]+))?  # all doc
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
                        + ((ch.sub_items.len() + 1) * 10) as u32,
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
                    weight = new_section_number.first().unwrap() * 1000
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

/// Generate llms.txt content for LLM discoverability.
///
/// Returns a markdown-formatted string containing:
/// - Project overview and features
/// - Installation instructions
/// - Quick start example
/// - Module list (all available modules)
/// - Lookup list
/// - Built-in variables
/// - Links to full documentation
pub fn generate_llms_txt() -> String {
    let sections: Vec<String> = vec![
        section_header(),
        section_what_is_rash(),
        section_installation(),
        section_quick_start(),
        section_modules(),
        section_lookups(),
        section_builtins(),
        section_links(),
    ];
    sections.join("\n")
}

fn section_header() -> String {
    let mut output = String::from("# Rash\n\n");
    output.push_str(
        "> Rash is a declarative shell scripting language using Ansible-like YAML syntax, ",
    );
    output.push_str(
        "compiled to a single Rust binary. Designed for container entrypoints, IoT devices, ",
    );
    output.push_str("and local scripting with zero dependencies.\n");
    output
}

fn section_what_is_rash() -> String {
    let mut output = String::from("\n## What is Rash\n\nRash provides:\n");
    output.push_str("- A **simple syntax** to maintain low complexity\n");
    output.push_str("- One static binary to be **container oriented**\n");
    output.push_str("- A **declarative** syntax to be idempotent\n");
    output.push_str("- **Clear output** to log properly\n");
    output.push_str("- **Security** by design\n");
    output.push_str("- **Speed and efficiency**\n");
    output.push_str("- **Modular** design\n");
    output.push_str("- Support of [MiniJinja](https://docs.rs/minijinja/latest/minijinja/syntax/index.html) **templates**\n");
    output
}

fn section_installation() -> String {
    let mut output = String::from("\n## Installation\n\n```bash\n");
    output.push_str("# Download latest binary (Linux/macOS)\n");
    output.push_str("curl -s https://api.github.com/repos/rash-sh/rash/releases/latest \\\n");
    output.push_str("    | grep browser_download_url \\\n");
    output.push_str("    | grep -v sha256 \\\n");
    output.push_str("    | grep $(uname -m) \\\n");
    output.push_str("    | grep $(uname | tr '[:upper:]' '[:lower:]') \\\n");
    output.push_str("    | grep -v musl \\\n");
    output.push_str("    | cut -d '\"' -f 4 \\\n");
    output.push_str("    | xargs curl -s -L \\\n");
    output.push_str("    | sudo tar xvz -C /usr/local/bin\n");
    output.push_str("```\n");
    output
}

fn section_quick_start() -> String {
    let mut output =
        String::from("\n## Quick Start\n\nCreate an `entrypoint.rh` file:\n\n```yaml\n");
    output.push_str("- name: Ensure directory exists\n");
    output.push_str("  file:\n");
    output.push_str("    path: /app/data\n");
    output.push_str("    state: directory\n\n");
    output.push_str("- name: Copy configuration\n");
    output.push_str("  copy:\n");
    output.push_str("    content: \"{{ env.APP_CONFIG }}\"\n");
    output.push_str("    dest: /app/config.yml\n\n");
    output.push_str("- name: Run application\n");
    output.push_str("  command:\n");
    output.push_str("    cmd: /app/bin/start\n");
    output.push_str("```\n");
    output
}

fn section_modules() -> String {
    let mut output =
        String::from("\n## Modules\n\nRash modules are idempotent operations like Ansible.\n\n");

    let mut modules: Vec<_> = MODULES.keys().collect();
    modules.sort();

    for name in modules {
        output.push_str(&format!("- {name}\n"));
    }

    output.push_str(&format!(
        "\nSee {DOCS_BASE_URL}/modules.html for full documentation.\n"
    ));
    output
}

fn section_lookups() -> String {
    let mut output =
        String::from("\n## Lookups\n\nLookups allow fetching data from external sources.\n\n");

    let mut lookups: Vec<_> = LOOKUPS.iter().collect();
    lookups.sort();

    for name in lookups {
        output.push_str(&format!("- {name}\n"));
    }

    output.push_str(&format!(
        "\nSee {DOCS_BASE_URL}/lookups.html for full documentation.\n"
    ));
    output
}

fn section_builtins() -> String {
    let mut output = String::from("\n## Built-in Variables\n\n");
    output.push_str("- `rash.path` - Path to the current script\n");
    output.push_str("- `rash.dir` - Directory of the current script\n");
    output.push_str("- `rash.cwd` - Current working directory\n");
    output.push_str("- `rash.arch` - System architecture\n");
    output.push_str("- `rash.check_mode` - Boolean indicating check mode\n");
    output.push_str("- `env.VAR_NAME` - Access environment variables\n");
    output.push_str(&format!(
        "\nSee {DOCS_BASE_URL}/builtins.html for full documentation.\n"
    ));
    output
}

fn section_links() -> String {
    format!("\n## Links\n\n- GitHub: {GITHUB_URL}\n- Documentation: {DOCS_BASE_URL}/\n")
}

#[cfg(test)]
mod llms_txt_test {
    use super::*;

    #[test]
    fn test_generate_llms_txt_contains_expected_sections() {
        let output = generate_llms_txt();
        assert!(output.contains("# Rash"), "Missing main header");
        assert!(
            output.contains("## What is Rash"),
            "Missing What is Rash section"
        );
        assert!(
            output.contains("## Installation"),
            "Missing Installation section"
        );
        assert!(
            output.contains("## Quick Start"),
            "Missing Quick Start section"
        );
        assert!(output.contains("## Modules"), "Missing Modules section");
        assert!(output.contains("## Lookups"), "Missing Lookups section");
        assert!(
            output.contains("## Built-in Variables"),
            "Missing Built-in Variables section"
        );
        assert!(output.contains("## Links"), "Missing Links section");
    }

    #[test]
    fn test_generate_llms_txt_contains_all_modules() {
        let output = generate_llms_txt();
        for name in MODULES.keys() {
            assert!(
                output.contains(&format!("- {name}\n")),
                "Missing module: {name}"
            );
        }
    }

    #[test]
    fn test_generate_llms_txt_contains_all_lookups() {
        let output = generate_llms_txt();
        for name in LOOKUPS.iter() {
            assert!(
                output.contains(&format!("- {name}\n")),
                "Missing lookup: {name}"
            );
        }
    }

    #[test]
    fn test_generate_llms_txt_links_are_valid() {
        let output = generate_llms_txt();
        assert!(output.contains(DOCS_BASE_URL), "Missing docs base URL");
        assert!(output.contains(GITHUB_URL), "Missing GitHub URL");
    }

    #[test]
    fn test_generate_llms_txt_quick_start_is_valid_yaml() {
        let output = generate_llms_txt();
        let start = output
            .find("```yaml\n")
            .expect("Could not find yaml block start");
        let end = output[start..]
            .find("\n```\n")
            .expect("Could not find yaml block end");
        let yaml_content = &output[start + 8..start + end];
        serde_norway::from_str::<serde_norway::Value>(yaml_content)
            .expect("Quick start example is not valid YAML");
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
