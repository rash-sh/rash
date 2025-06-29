/// ANCHOR: lookup
/// # file
///
/// Read file contents from the filesystem.
///
/// ## Parameters
///
/// | Parameter | Required | Type    | Values | Description                                                                         |
/// | --------- | -------- | ------- | ------ | ----------------------------------------------------------------------------------- |
/// | path      | yes      | string  |        | Path(s) of files to read                                                           |
/// | lstrip    | no       | boolean | true/false | Whether or not to remove whitespace from the beginning of the file. **[default: `false`]** |
/// | rstrip    | no       | boolean | true/false | Whether or not to remove whitespace from the ending of the file. **[default: `true`]** |
///
/// ## Notes
///
/// - This lookup returns the contents from a file on the local filesystem.
/// - When keyword and positional parameters are used together, positional parameters must be listed before keyword parameters.
/// - If read in variable context, the file can be interpreted as YAML if the content is valid to the parser.
/// - This lookup does not understand 'globbing', use the fileglob lookup instead.
/// - The file must be readable by the user running the script.
///
/// ANCHOR_END: lookup
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Read a file
///   debug:
///     msg: "{{ file('/etc/hostname') }}"
///
/// - name: Read file without stripping whitespace
///   debug:
///     msg: "{{ file('/tmp/data.txt', rstrip=false) }}"
///
/// - name: Read file and strip whitespace from both ends
///   debug:
///     msg: "{{ file('/tmp/config.txt', lstrip=true, rstrip=true) }}"
///
/// - name: Read multiple files in a loop
///   debug:
///     msg: "{{ item }}"
///   loop:
///     - "{{ file('/etc/hostname') }}"
///     - "{{ file('/etc/os-release') }}"
/// ```
/// ANCHOR_END: examples
use std::fs;
use std::result::Result as StdResult;

use log::trace;
use minijinja::{Error as MinijinjaError, ErrorKind as MinijinjaErrorKind, Value, value::Kwargs};

pub fn function(path: String, options: Kwargs) -> StdResult<Value, MinijinjaError> {
    trace!("file lookup - reading file: '{path}'");

    // Get optional parameters
    let lstrip: bool = options.get::<Option<bool>>("lstrip")?.unwrap_or(false);
    let rstrip: bool = options.get::<Option<bool>>("rstrip")?.unwrap_or(true);

    trace!("file lookup - lstrip: {lstrip}, rstrip: {rstrip}");

    // Read file contents
    let content = fs::read_to_string(&path).map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to read file '{path}': {e}"),
        )
    })?;

    // Apply stripping based on parameters
    let result = match (lstrip, rstrip) {
        (true, true) => content.trim().to_string(),
        (true, false) => content.trim_start().to_string(),
        (false, true) => content.trim_end().to_string(),
        (false, false) => content,
    };

    trace!("file lookup - returning {} characters", result.len());

    // Ensure all options were used
    options.assert_all_used()?;

    Ok(Value::from(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::Value;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_read_basic() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "hello world").unwrap();

        // Create empty kwargs
        let kwargs = Kwargs::from_iter(std::iter::empty::<(&str, Value)>());

        let result = function(temp_file.path().to_string_lossy().to_string(), kwargs);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str().unwrap(), "hello world");
    }

    #[test]
    fn test_file_read_with_whitespace() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "  hello world  ").unwrap();

        // Default behavior (rstrip=true, lstrip=false)
        let kwargs = Kwargs::from_iter(std::iter::empty::<(&str, Value)>());

        let result = function(temp_file.path().to_string_lossy().to_string(), kwargs);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str().unwrap(), "  hello world");
    }

    #[test]
    fn test_file_read_lstrip() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "  hello world  ").unwrap();

        let kwargs = Kwargs::from_iter([("lstrip", Value::from(true))]);

        let result = function(temp_file.path().to_string_lossy().to_string(), kwargs);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str().unwrap(), "hello world");
    }

    #[test]
    fn test_file_read_no_strip() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "  hello world  ").unwrap();

        let kwargs = Kwargs::from_iter([("rstrip", Value::from(false))]);

        let result = function(temp_file.path().to_string_lossy().to_string(), kwargs);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str().unwrap(), "  hello world  \n");
    }

    #[test]
    fn test_file_not_found() {
        let kwargs = Kwargs::from_iter(std::iter::empty::<(&str, Value)>());

        let result = function("/nonexistent/file.txt".to_string(), kwargs);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read file")
        );
    }
}
