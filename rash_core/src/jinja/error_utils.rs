use crate::error::{Error, ErrorKind};
use minijinja::Value;
use minijinja::{Error as MiniJinjaError, ErrorKind as MiniJinjaErrorKind};
use regex::{Captures, Regex};
use std::sync::LazyLock;

/// Extract variable name from template that caused undefined error
pub fn extract_undefined_variable_from_template(template: &str, vars: &Value) -> Option<String> {
    // Simple regex to find Jinja variable references
    static VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*)\s*(?:\|[^}]*)?\}\}",
        )
        .unwrap()
    });

    // Find all variable references in the template and check which ones are undefined
    for cap in VAR_REGEX.captures_iter(template) {
        if let Some(var_match) = cap.get(1) {
            let var_name = var_match.as_str();
            // Skip built-in functions and filters
            if !var_name.starts_with("range") && !var_name.starts_with("debug") {
                // Check if this variable is defined in the context
                if is_variable_undefined(var_name, vars) {
                    return Some(var_name.to_string());
                }
            }
        }
    }

    None
}

/// Check if a variable is undefined in the given context
pub fn is_variable_undefined(var_name: &str, vars: &Value) -> bool {
    let parts: Vec<&str> = var_name.split('.').collect();
    let mut current_value = vars.clone();

    for part in parts {
        match current_value.get_attr(part) {
            Ok(value) if !value.is_undefined() => {
                current_value = value;
            }
            _ => return true,
        }
    }

    false
}

/// Comprehensive error handler for minijinja errors with template context
pub fn handle_template_error(e: MiniJinjaError, template: &str, vars: &Value) -> Error {
    let context_msg = match e.kind() {
        MiniJinjaErrorKind::UndefinedError => {
            let variable_name = extract_undefined_variable_from_template(template, vars);
            if let Some(var_name) = variable_name {
                format!("undefined variable '{var_name}' in template: {template}")
            } else {
                format!("undefined variable in template: {template}")
            }
        }
        _ => enhance_template_text(&e.to_string(), template),
    };
    Error::new(ErrorKind::JinjaRenderError, context_msg)
}

fn enhance_template_text(error_msg: &str, template: &str) -> String {
    static TEMPLATE_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\(in\s\w+:[0-9]*\)").unwrap());

    TEMPLATE_REGEX
        .replace_all(error_msg, |_caps: &Captures| {
            format!("in template: {template}")
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    #[test]
    fn test_extract_undefined_variable_simple() {
        let template = "{{ undefined_var }}";
        let vars = context! {};
        let result = extract_undefined_variable_from_template(template, &vars);
        assert_eq!(result, Some("undefined_var".to_string()));
    }

    #[test]
    fn test_extract_undefined_variable_with_defined() {
        let template = "{{ defined_var }} and {{ undefined_var }}";
        let vars = context! { defined_var => "hello" };
        let result = extract_undefined_variable_from_template(template, &vars);
        assert_eq!(result, Some("undefined_var".to_string()));
    }

    #[test]
    fn test_extract_undefined_variable_nested() {
        let template = "{{ foo.bar }}";
        let vars = context! {};
        let result = extract_undefined_variable_from_template(template, &vars);
        assert_eq!(result, Some("foo.bar".to_string()));
    }

    #[test]
    fn test_is_variable_undefined_simple() {
        let vars = context! { defined_var => "value" };
        assert!(is_variable_undefined("undefined_var", &vars));
        assert!(!is_variable_undefined("defined_var", &vars));
    }

    #[test]
    fn test_is_variable_undefined_nested() {
        use serde_json::json;
        let vars = context! {
            obj => json!({
                "prop": "value"
            })
        };
        assert!(!is_variable_undefined("obj.prop", &vars));
        assert!(is_variable_undefined("obj.missing", &vars));
        assert!(is_variable_undefined("missing.prop", &vars));
    }

    #[test]
    fn test_comprehensive_error_handling() {
        use minijinja::{Environment, UndefinedBehavior};

        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);

        // Test cases for different error types
        let test_cases = vec![
            // UndefinedError cases
            ("{{ undefined_var }}", "undefined variable"),
            ("{{ obj.missing_prop }}", "undefined variable"),
            // InvalidOperation cases
            ("{{ 'text' | int }}", "invalid operation"),
            ("{{ 'text' | float }}", "invalid operation"),
        ];

        for (template_str, expected_msg_part) in test_cases {
            println!("Testing template: {template_str}");

            // Add template - this might fail for syntax errors
            if let Ok(()) = env.add_template("test", template_str) {
                let template = env.get_template("test").unwrap();

                match template.render(context! { value => "text" }) {
                    Ok(_) => {
                        // This template succeeded, skip
                        continue;
                    }
                    Err(minijinja_error) => {
                        let rash_error =
                            handle_template_error(minijinja_error, template_str, &context! {});
                        let error_msg = rash_error.to_string().to_lowercase();

                        // Check that our error enhancement worked
                        assert!(
                            error_msg.contains(expected_msg_part),
                            "Error message '{error_msg}' should contain '{expected_msg_part}'"
                        );
                        assert!(
                            error_msg.contains("template:"),
                            "Error message should contain template context: {error_msg}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_error_kinds_coverage() {
        // Test that we handle all the common error types appropriately
        use minijinja::{Environment, UndefinedBehavior};

        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);

        // Test UndefinedError
        env.add_template("undefined", "{{ missing }}").unwrap();
        let template = env.get_template("undefined").unwrap();
        if let Err(minijinja_error) = template.render(context! {}) {
            let rash_error = handle_template_error(minijinja_error, "{{ missing }}", &context! {});
            assert!(
                rash_error
                    .to_string()
                    .contains("undefined variable 'missing'")
            );
        }

        // Test InvalidOperation
        env.add_template("invalid_op", "{{ 'text' | int }}")
            .unwrap();
        let template = env.get_template("invalid_op").unwrap();
        if let Err(minijinja_error) = template.render(context! {}) {
            let rash_error =
                handle_template_error(minijinja_error, "{{ 'text' | int }}", &context! {});
            assert!(rash_error.to_string().contains("invalid operation"));
        }
    }

    #[test]
    fn test_enhance_template_text() {
        let error_msg = "An error occurred (in t:1)";
        let template = "{{ foo }}";

        let enhanced_msg = enhance_template_text(error_msg, template);
        assert_eq!(enhanced_msg, "An error occurred in template: {{ foo }}");

        let error_msg = "An error occurred (in test:2)";
        let template = "{{ foo }}";

        let enhanced_msg = enhance_template_text(error_msg, template);
        assert_eq!(enhanced_msg, "An error occurred in template: {{ foo }}");
    }
}
