use crate::error::{Error, ErrorKind, Result};

/// Get the width of the terminal.
///
/// This function attempts to determine the terminal width using multiple approaches:
/// 1. Environment variables: Check COLUMNS and TERM_WIDTH
/// 2. Use ioctl system call to get terminal size directly
/// 3. Try tput command as fallback
/// 4. Fallback to 80 columns
pub fn get_terminal_width() -> usize {
    // Try environment variables first
    for env_var in ["COLUMNS", "TERM_WIDTH"] {
        if let Ok(columns) = std::env::var(env_var) {
            if let Ok(width) = columns.parse::<usize>() {
                if width > 0 {
                    return width;
                }
            }
        }
    }

    // Try to get terminal size using direct ioctl system call
    #[cfg(unix)]
    {
        if let Some(width) = get_terminal_width_ioctl() {
            return width;
        }
    }

    // Try to get terminal size using tput command as fallback
    #[cfg(unix)]
    {
        if let Some(width) = get_terminal_width_tput() {
            return width;
        }
    }

    // Default fallback
    80
}

#[cfg(unix)]
fn get_terminal_width_tput() -> Option<usize> {
    use std::process::Command;

    // Try to use tput command to get terminal width
    if let Ok(output) = Command::new("tput").arg("cols").output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(width) = output_str.trim().parse::<usize>() {
                if width > 0 {
                    return Some(width);
                }
            }
        }
    }

    None
}

#[cfg(unix)]
fn get_terminal_width_ioctl() -> Option<usize> {
    use std::mem;
    use std::os::fd::{AsRawFd, RawFd};

    // Define winsize struct that matches system struct
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct WinSize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }

    // TIOCGWINSZ ioctl request constant
    use nix::libc::{TIOCGWINSZ, ioctl};

    // Try stdout, stderr, then stdin
    let fds: [RawFd; 3] = [
        std::io::stdout().as_raw_fd(),
        std::io::stderr().as_raw_fd(),
        std::io::stdin().as_raw_fd(),
    ];

    for &fd in &fds {
        let mut ws: WinSize = unsafe { mem::zeroed() };

        // Use nix crate's ioctl wrapper with proper type casting
        let result = unsafe { ioctl(fd, TIOCGWINSZ as _, &mut ws as *mut WinSize as *mut _) };

        if result == 0 && ws.ws_col > 0 {
            return Some(ws.ws_col as usize);
        }
    }

    None
}

pub fn parse_octal(s: &str) -> Result<u32> {
    match s.len() {
        3 => u32::from_str_radix(s, 8).map_err(|e| Error::new(ErrorKind::InvalidData, e)),
        4 => u32::from_str_radix(s.get(1..).unwrap(), 8)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e)),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{s} cannot be parsed to octal"),
        )),
    }
}

pub fn merge_json(a: &mut serde_json::Value, b: serde_json::Value) {
    if let (Some(a_map), Some(b_map)) = (a.as_object_mut(), b.as_object()) {
        for (k, v) in b_map {
            match (a_map.get_mut(k), &v) {
                (Some(serde_json::Value::Array(a_arr)), serde_json::Value::Array(b_arr)) => {
                    a_arr.extend(b_arr.clone());
                }
                (Some(serde_json::Value::Number(a_num)), serde_json::Value::Number(b_num))
                    if a_num.is_u64() && b_num.is_u64() =>
                {
                    let sum = a_num.as_u64().unwrap() + b_num.as_u64().unwrap();
                    a_map.insert(k.to_string(), serde_json::Value::from(sum));
                }
                (Some(a_val), _) => {
                    merge_json(a_val, v.clone());
                }
                (None, _) => {
                    a_map.insert(k.to_string(), v.clone());
                }
            }
        }
    } else {
        *a = b;
    }
}

pub fn default_false() -> Option<bool> {
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_octal() {
        assert_eq!(parse_octal("644").unwrap(), 0o644);
        assert_eq!(parse_octal("0644").unwrap(), 0o644);
        assert_eq!(parse_octal("777").unwrap(), 0o777);
        assert_eq!(parse_octal("0444").unwrap(), 0o444);
        assert_eq!(parse_octal("600").unwrap(), 0o600);
        assert_eq!(parse_octal("0600").unwrap(), 0o600);
    }

    #[test]
    fn test_merge() {
        let mut a = json!({ "a": { "b": "foo" } });
        let b = json!({ "a": { "c": "boo" } });
        merge_json(&mut a, b);

        assert_eq!(a.get("a").unwrap(), &json!({ "b": "foo", "c": "boo" }));
    }

    #[test]
    fn test_merge_overlapping() {
        let mut a = json!({ "a": "foo" });
        let b = json!({ "a": "boo" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("boo"));
    }

    #[test]
    fn test_merge_overlapping_nested() {
        let mut a = json!({ "a": { "c": "boo" } });
        let b = json!({ "a": { "b": "foo" } });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!({ "b": "foo", "c": "boo" }));
    }

    #[test]
    fn test_merge_mixed_types() {
        let mut a = json!({ "a": "simple_value" });
        let b = json!({ "a": { "b": "foo" } });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!({ "b": "foo" }));
    }

    #[test]
    fn test_merge_with_empty() {
        let mut a = json!({});
        let b = json!({ "a": "foo" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("foo"));

        let mut a = json!({ "a": "foo" });
        let b = json!({});
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("foo"));
    }

    #[test]
    fn test_merge_deeply_nested() {
        let mut a = json!({ "a": { "b": { "d": "boo" } } });
        let b = json!({ "a": { "b": { "c": "foo" } } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap(),
            &json!({ "b": { "c": "foo", "d": "boo" } })
        );
    }

    #[test]
    fn test_merge_deeply_nested_partially_overlap() {
        let mut a = json!({ "a": { "b": { "d": "boo", "e": "world" } } });
        let b = json!({ "a": { "b": { "c": "foo", "e": "hello" } } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap(),
            &json!({ "b": { "c": "foo", "d": "boo", "e": "hello" } })
        );
    }

    #[test]
    fn test_merge_add_top_level() {
        let mut a = json!({ "a": "foo" });
        let b = json!({ "b": "boo" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("foo"));
        assert_eq!(a.get("b").unwrap(), &json!("boo"));
    }

    #[test]
    fn test_merge_both_empty() {
        let mut a = json!({});
        let b = json!({});
        merge_json(&mut a, b);
        assert_eq!(a, json!({}));
    }

    #[test]
    fn test_merge_seq_concatenation() {
        let mut a = json!({ "a": vec![1, 2, 3] });
        let b = json!({ "a": vec![4, 5, 6] });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!(vec![1, 2, 3, 4, 5, 6]));
    }

    #[test]
    fn test_merge_seq_with_non_seq() {
        let mut a = json!({ "a": vec![1, 2, 3] });
        let b = json!({ "a": "override" });
        merge_json(&mut a, b);
        assert_eq!(a.get("a").unwrap(), &json!("override"));
    }

    #[test]
    fn test_merge_nested_seq_concatenation() {
        let mut a = json!({ "a": { "b": vec![1, 2] } });
        let b = json!({ "a": { "b": vec![3, 4] } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap().get("b").unwrap(),
            &json!(vec![1, 2, 3, 4])
        );
    }

    #[test]
    fn test_merge_deeply_nested_mixed_with_seq() {
        let mut a = json!({ "a": { "b": { "c": vec![1, 2], "e": "hello" } } });
        let b = json!({ "a": { "b": { "c": vec![3, 4], "e": "world" } } });
        merge_json(&mut a, b);
        assert_eq!(
            a.get("a").unwrap(),
            &json!({ "b": { "c": vec![1, 2, 3, 4], "e": "world" } })
        );
    }

    #[test]
    fn test_get_terminal_width() {
        // Test that get_terminal_width returns a reasonable value
        let width = get_terminal_width();
        assert!(
            width >= 80,
            "Terminal width should be at least 80, got {}",
            width
        );
        assert!(
            width <= 1000,
            "Terminal width should be reasonable, got {}",
            width
        );
    }

    #[test]
    fn test_get_terminal_width_with_env() {
        // Test with COLUMNS environment variable
        unsafe {
            std::env::set_var("COLUMNS", "120");
        }
        let width = get_terminal_width();
        // Clean up before assertion to avoid affecting other tests
        unsafe {
            std::env::remove_var("COLUMNS");
        }
        assert_eq!(width, 120);
    }

    #[test]
    fn test_get_terminal_width_fallback() {
        // Test fallback behavior with invalid env var
        unsafe {
            std::env::set_var("TERM_WIDTH", "invalid");
        }
        let width = get_terminal_width();
        // Should fall back to 80 or get actual terminal width
        assert!(width >= 80);
        unsafe {
            std::env::remove_var("TERM_WIDTH");
        }
    }
}
