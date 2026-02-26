/// ANCHOR: module
/// # xml
///
/// Manage settings in XML configuration files.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - xml:
///     path: /etc/app/config.xml
///     xpath: /config/server/port
///     value: "8080"
///
/// - xml:
///     path: /etc/app/config.xml
///     xpath: /config/database
///     attribute: timeout
///     value: "30"
///
/// - xml:
///     path: /etc/app/config.xml
///     xpath: /config/debug
///     state: absent
///
/// - xml:
///     path: /etc/app/config.xml
///     xpath: /config/logging/level
///     value: "INFO"
///     pretty_print: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::prelude::*;
use std::path::Path;

use minijinja::Value;
use quick_xml::Reader;
use quick_xml::events::Event;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The absolute path to the XML file to modify.
    pub path: String,
    /// The xpath expression to select elements. Supports simple path notation like /config/server/port
    pub xpath: String,
    /// The value to set for the element or attribute. Required if state=present.
    pub value: Option<String>,
    /// The attribute name to modify. If not specified, modifies element text content.
    pub attribute: Option<String>,
    /// Whether the element/attribute should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Format the output XML with proper indentation.
    /// **[default: `true`]**
    pub pretty_print: Option<bool>,
    /// Create a backup file before modifying.
    /// **[default: `false`]**
    pub backup: Option<bool>,
}

#[derive(Debug, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone)]
struct XmlNode {
    name: String,
    text: Option<String>,
    attributes: HashMap<String, String>,
    children: Vec<XmlNode>,
}

impl XmlNode {
    fn new(name: String) -> Self {
        Self {
            name,
            text: None,
            attributes: HashMap::new(),
            children: Vec::new(),
        }
    }

    fn get_or_create_child(&mut self, name: &str) -> usize {
        if let Some(pos) = self.children.iter().position(|c| c.name == name) {
            return pos;
        }
        self.children.push(XmlNode::new(name.to_string()));
        self.children.len() - 1
    }
}

fn parse_xpath(xpath: &str) -> Vec<&str> {
    let trimmed = xpath.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return Vec::new();
    }
    trimmed.trim_start_matches('/').split('/').collect()
}

fn parse_xml_to_tree(content: &str) -> Result<XmlNode> {
    let mut reader = Reader::from_str(content);
    let mut root = XmlNode::new("__root__".to_string());
    let mut path: Vec<usize> = Vec::new();
    let mut text_buffer = String::new();

    fn get_node_at_path<'a>(root: &'a mut XmlNode, path: &[usize]) -> &'a mut XmlNode {
        let mut current = root;
        for &idx in path {
            current = &mut current.children[idx];
        }
        current
    }

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                let parent = get_node_at_path(&mut root, &path);
                let idx = parent.get_or_create_child(&name);
                path.push(idx);

                let node = get_node_at_path(&mut root, &path);

                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let value = String::from_utf8_lossy(&attr.value).to_string();
                    node.attributes.insert(key, value);
                }

                text_buffer.clear();
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut node = XmlNode::new(name);

                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let value = String::from_utf8_lossy(&attr.value).to_string();
                    node.attributes.insert(key, value);
                }

                let parent = get_node_at_path(&mut root, &path);
                if !parent.children.iter().any(|c| c.name == node.name) {
                    parent.children.push(node);
                }
            }
            Ok(Event::End(_)) => {
                if !text_buffer.trim().is_empty() {
                    let node = get_node_at_path(&mut root, &path);
                    node.text = Some(text_buffer.trim().to_string());
                }
                text_buffer.clear();
                path.pop();
            }
            Ok(Event::Text(e)) => {
                text_buffer.push_str(&String::from_utf8_lossy(&e));
            }
            Ok(Event::CData(e)) => {
                text_buffer.push_str(&String::from_utf8_lossy(&e));
            }
            Ok(Event::Decl(_)) => {}
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("XML parsing error: {:?}", e),
                ));
            }
            _ => {}
        }
    }

    Ok(root)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn serialize_node(node: &XmlNode, indent: usize, pretty: bool) -> String {
    let indent_str = if pretty {
        "  ".repeat(indent)
    } else {
        String::new()
    };
    let newline = if pretty { "\n" } else { "" };
    let mut result = String::new();

    result.push_str(&indent_str);
    result.push('<');
    result.push_str(&node.name);

    for (key, value) in &node.attributes {
        result.push_str(&format!(" {}=\"{}\"", key, escape_xml(value)));
    }

    if node.children.is_empty() && node.text.is_none() {
        result.push_str("/>");
        result.push_str(newline);
    } else if node.children.is_empty() {
        result.push('>');
        if let Some(text) = &node.text {
            result.push_str(&escape_xml(text));
        }
        result.push_str(&format!("</{}>", node.name));
        result.push_str(newline);
    } else {
        result.push('>');
        result.push_str(newline);

        for child in &node.children {
            result.push_str(&serialize_node(child, indent + 1, pretty));
        }

        result.push_str(&indent_str);
        result.push_str(&format!("</{}>", node.name));
        result.push_str(newline);
    }

    result
}

fn serialize_tree(root: &XmlNode, pretty: bool) -> String {
    let mut result = String::new();
    result.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    for child in &root.children {
        result.push_str(&serialize_node(child, 0, pretty));
    }
    result
}

fn find_node_idx(root: &XmlNode, path_parts: &[&str]) -> Option<Vec<usize>> {
    if path_parts.is_empty() {
        return None;
    }

    let mut current = root;
    let mut path = Vec::new();

    for part in path_parts {
        match current.children.iter().position(|c| c.name == *part) {
            Some(idx) => {
                path.push(idx);
                current = &current.children[idx];
            }
            None => return None,
        }
    }

    Some(path)
}

fn get_node_at_path_mut<'a>(root: &'a mut XmlNode, path: &[usize]) -> &'a mut XmlNode {
    let mut current = root;
    for &idx in path {
        current = &mut current.children[idx];
    }
    current
}

fn find_or_create_path(root: &mut XmlNode, path_parts: &[&str]) -> Vec<usize> {
    if path_parts.is_empty() {
        return Vec::new();
    }

    let mut current = root;
    let mut path = Vec::new();

    for part in path_parts {
        let idx = current.get_or_create_child(part);
        path.push(idx);
        current = &mut current.children[idx];
    }

    path
}

fn remove_node(root: &mut XmlNode, path_parts: &[&str]) -> bool {
    if path_parts.is_empty() {
        return false;
    }

    if path_parts.len() == 1 {
        let name = path_parts[0];
        let len_before = root.children.len();
        root.children.retain(|c| c.name != name);
        return root.children.len() < len_before;
    }

    let parent_path = &path_parts[..path_parts.len() - 1];
    let child_name = path_parts[path_parts.len() - 1];

    if let Some(parent_path_idx) = find_node_idx(root, parent_path) {
        let parent = get_node_at_path_mut(root, &parent_path_idx);
        let len_before = parent.children.len();
        parent.children.retain(|c| c.name != child_name);
        return parent.children.len() < len_before;
    }

    false
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

pub fn xml(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let pretty_print = params.pretty_print.unwrap_or(true);
    let backup = params.backup.unwrap_or(false);

    if state == State::Present && params.value.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        ));
    }

    let path = Path::new(&params.path);
    let path_parts = parse_xpath(&params.xpath);

    if path_parts.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "xpath cannot be empty or just '/'",
        ));
    }

    let original_content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut root = if original_content.trim().is_empty() {
        XmlNode::new("__root__".to_string())
    } else {
        parse_xml_to_tree(&original_content)?
    };

    let mut changed = false;

    match state {
        State::Present => {
            let value = params.value.as_ref().unwrap();
            let node_path = find_or_create_path(&mut root, &path_parts);
            let node = get_node_at_path_mut(&mut root, &node_path);

            if let Some(ref attr) = params.attribute {
                match node.attributes.get(attr) {
                    Some(existing) if existing == value => {}
                    _ => {
                        node.attributes.insert(attr.clone(), value.clone());
                        changed = true;
                    }
                }
            } else {
                match &node.text {
                    Some(existing) if existing == value => {}
                    _ => {
                        node.text = Some(value.clone());
                        changed = true;
                    }
                }
            }
        }
        State::Absent => {
            if remove_node(&mut root, &path_parts) {
                changed = true;
            }
        }
    }

    if changed {
        let new_content = serialize_tree(&root, pretty_print);

        diff(&original_content, &new_content);

        if !check_mode {
            if backup && path.exists() {
                let backup_path = format!("{}.{}", params.path, timestamp());
                fs::copy(path, &backup_path)?;
            }

            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            file.write_all(new_content.as_bytes())?;
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(params.path),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Xml;

impl Module for Xml {
    fn get_name(&self) -> &str {
        "xml"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((xml(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "/etc/config.xml"
            xpath: "/config/server/port"
            value: "8080"
            state: "present"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/etc/config.xml".to_owned(),
                xpath: "/config/server/port".to_owned(),
                value: Some("8080".to_owned()),
                attribute: None,
                state: Some(State::Present),
                pretty_print: None,
                backup: None,
            }
        );
    }

    #[test]
    fn test_parse_xpath() {
        assert_eq!(parse_xpath("/config/server"), vec!["config", "server"]);
        assert_eq!(parse_xpath("config/server"), vec!["config", "server"]);
        assert_eq!(parse_xpath("/"), Vec::<&str>::new());
        assert_eq!(parse_xpath(""), Vec::<&str>::new());
    }

    #[test]
    fn test_xml_set_element_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <server>
    <port>80</port>
  </server>
</config>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/server/port".to_string(),
            value: Some("8080".to_string()),
            attribute: None,
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("8080"));
    }

    #[test]
    fn test_xml_set_attribute() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <database host="localhost"/>
</config>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/database".to_string(),
            value: Some("db.example.com".to_string()),
            attribute: Some("host".to_string()),
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("host=\"db.example.com\""));
    }

    #[test]
    fn test_xml_add_new_attribute() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <database host="localhost"/>
</config>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/database".to_string(),
            value: Some("30".to_string()),
            attribute: Some("timeout".to_string()),
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("timeout=\"30\""));
    }

    #[test]
    fn test_xml_add_new_element() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config/>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/server/port".to_string(),
            value: Some("8080".to_string()),
            attribute: None,
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("<port>8080</port>"));
    }

    #[test]
    fn test_xml_remove_element() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <debug>true</debug>
  <server>
    <port>8080</port>
  </server>
</config>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/debug".to_string(),
            value: None,
            attribute: None,
            state: Some(State::Absent),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("<debug>"));
        assert!(content.contains("<port>8080</port>"));
    }

    #[test]
    fn test_xml_remove_nonexistent() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <server/>
</config>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/nonexistent".to_string(),
            value: None,
            attribute: None,
            state: Some(State::Absent),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_xml_no_change_same_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <port>8080</port>
</config>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/port".to_string(),
            value: Some("8080".to_string()),
            attribute: None,
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_xml_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <port>80</port>
</config>
"#,
        )
        .unwrap();
        let original_content = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/port".to_string(),
            value: Some("8080".to_string()),
            attribute: None,
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, true).unwrap();
        assert!(result.changed);

        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_xml_missing_value_for_present() {
        let params = Params {
            path: "/tmp/test.xml".to_string(),
            xpath: "/config/port".to_string(),
            value: None,
            attribute: None,
            state: Some(State::Present),
            pretty_print: None,
            backup: None,
        };

        let result = xml(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("value parameter is required")
        );
    }

    #[test]
    fn test_xml_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/port".to_string(),
            value: Some("8080".to_string()),
            attribute: None,
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: None,
        };

        let result = xml(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("<?xml"));
        assert!(content.contains("<port>8080</port>"));
    }

    #[test]
    fn test_xml_backup() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.xml");

        fs::write(
            &file_path,
            r#"<?xml version="1.0"?>
<config>
  <port>80</port>
</config>
"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            xpath: "/config/port".to_string(),
            value: Some("8080".to_string()),
            attribute: None,
            state: Some(State::Present),
            pretty_print: Some(true),
            backup: Some(true),
        };

        let result = xml(params, false).unwrap();
        assert!(result.changed);

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("config.xml."))
            .collect();
        assert_eq!(backup_files.len(), 1);

        let backup_content = fs::read_to_string(backup_files[0].path()).unwrap();
        assert!(backup_content.contains("<port>80</port>"));
    }

    #[test]
    fn test_xml_empty_xpath_error() {
        let params = Params {
            path: "/tmp/test.xml".to_string(),
            xpath: "/".to_string(),
            value: Some("8080".to_string()),
            attribute: None,
            state: Some(State::Present),
            pretty_print: None,
            backup: None,
        };

        let result = xml(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
        assert_eq!(escape_xml("a'b"), "a&apos;b");
        assert_eq!(escape_xml("a\"b"), "a&quot;b");
    }
}
