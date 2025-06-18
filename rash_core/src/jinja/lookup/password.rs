/// ANCHOR: lookup
/// # password
///
/// Generate a random plaintext password and store it in a file at a given filepath.
///
/// ## Parameters
///
/// | Parameter | Required | Type    | Values | Description                                                                         |
/// | --------- | -------- | ------- | ------ | ----------------------------------------------------------------------------------- |
/// | path      | yes      | string  |        | Path to the file that stores/will store the password                               |
/// | length    | no       | integer |        | The length of the generated password. **[default: `20`]**                         |
/// | chars     | no       | array   |        | Character sets to use for password generation. **[default: `['ascii_letters', 'digits', 'punctuation']`]** |
/// | seed      | no       | string  |        | A seed to initialize the random number generator for idempotent passwords          |
///
/// ## Notes
///
/// - If the file exists previously, it will retrieve its contents (behaves like reading a file).
/// - Usage of /dev/null as a path generates a new random password each time without storing it.
/// - The file must be readable by the user running the script, or the user must have sufficient privileges to create it.
/// - Empty files cause the password to return as an empty string.
///
/// ANCHOR_END: lookup
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Generate or retrieve a password
///   debug:
///     msg: "Password: {{ password('/tmp/mypassword') }}"
///
/// - name: Generate a short password with only letters
///   debug:
///     msg: "Simple password: {{ password('/tmp/simple', length=8, chars=['ascii_letters']) }}"
///
/// - name: Generate a digits-only password
///   debug:
///     msg: "PIN: {{ password('/tmp/pin', length=4, chars=['digits']) }}"
///
/// - name: Generate temporary password (not stored)
///   debug:
///     msg: "Temp password: {{ password('/dev/null', length=12) }}"
///
/// - name: Generate idempotent password with seed
///   debug:
///     msg: "Seeded password: {{ password('/dev/null', seed='my-seed', length=16) }}"
/// ```
/// ANCHOR_END: examples
use std::fs;
use std::io::Write;
use std::path::Path;
use std::result::Result as StdResult;

use minijinja::{Error as MinijinjaError, ErrorKind as MinijinjaErrorKind, Value, value::Kwargs};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

const DEFAULT_LENGTH: usize = 20;
const ASCII_LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const ASCII_UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const PUNCTUATION: &str = ".,:-_";

pub fn function(path: String, options: Kwargs) -> StdResult<Value, MinijinjaError> {
    // Check if file exists and is not /dev/null
    if path != "/dev/null" && Path::new(&path).exists() {
        // Read existing password from file
        match fs::read_to_string(&path) {
            Ok(content) => return Ok(Value::from(content.trim_end())),
            Err(e) => {
                return Err(MinijinjaError::new(
                    MinijinjaErrorKind::InvalidOperation,
                    format!("Failed to read password file '{}': {}", path, e),
                ));
            }
        }
    }

    // Generate new password
    let length: usize = options
        .get::<Option<usize>>("length")?
        .unwrap_or(DEFAULT_LENGTH);
    let chars: Option<Vec<String>> = options.get("chars")?;
    let seed: Option<String> = options.get("seed")?;

    // Build character set
    let charset = build_charset(chars.as_ref())?;

    // Generate password
    let password = if let Some(seed_value) = seed {
        generate_seeded_password(&charset, length, &seed_value)?
    } else {
        generate_random_password(&charset, length)?
    };

    // Store password if not /dev/null
    if path != "/dev/null" {
        if let Some(parent) = Path::new(&path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    MinijinjaError::new(
                        MinijinjaErrorKind::InvalidOperation,
                        format!("Failed to create directory for password file: {}", e),
                    )
                })?;
            }
        }

        let mut file = fs::File::create(&path).map_err(|e| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("Failed to create password file '{}': {}", path, e),
            )
        })?;

        file.write_all(password.as_bytes()).map_err(|e| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("Failed to write password to file '{}': {}", path, e),
            )
        })?;
    }

    options.assert_all_used()?;
    Ok(Value::from(password))
}

fn build_charset(chars: Option<&Vec<String>>) -> StdResult<String, MinijinjaError> {
    let default_chars = vec![
        "ascii_letters".to_string(),
        "digits".to_string(),
        "punctuation".to_string(),
    ];
    let char_sets = chars.unwrap_or(&default_chars);

    let mut charset = String::new();

    for char_set in char_sets {
        match char_set.as_str() {
            "ascii_lowercase" => charset.push_str(ASCII_LOWERCASE),
            "ascii_uppercase" => charset.push_str(ASCII_UPPERCASE),
            "ascii_letters" => {
                charset.push_str(ASCII_LOWERCASE);
                charset.push_str(ASCII_UPPERCASE);
            }
            "digits" => charset.push_str(DIGITS),
            "punctuation" => charset.push_str(PUNCTUATION),
            other => {
                // Treat as literal characters
                charset.push_str(other);
            }
        }
    }

    if charset.is_empty() {
        return Err(MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            "No valid characters specified for password generation",
        ));
    }

    Ok(charset)
}

fn generate_random_password(charset: &str, length: usize) -> StdResult<String, MinijinjaError> {
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = charset.chars().collect();

    let password: String = (0..length)
        .map(|_| *chars.choose(&mut rng).unwrap())
        .collect();

    Ok(password)
}

fn generate_seeded_password(
    charset: &str,
    length: usize,
    seed: &str,
) -> StdResult<String, MinijinjaError> {
    // Create a deterministic seed from the string
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(seed, &mut hasher);
    let seed_value = std::hash::Hasher::finish(&hasher);

    let mut rng = StdRng::seed_from_u64(seed_value);
    let chars: Vec<char> = charset.chars().collect();

    let password: String = (0..length)
        .map(|_| *chars.choose(&mut rng).unwrap())
        .collect();

    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_charset_default() {
        let charset = build_charset(None).unwrap();
        assert!(charset.contains("a")); // ascii_lowercase
        assert!(charset.contains("A")); // ascii_uppercase
        assert!(charset.contains("0")); // digits
        assert!(charset.contains(".")); // punctuation
    }

    #[test]
    fn test_build_charset_digits_only() {
        let chars = vec!["digits".to_string()];
        let charset = build_charset(Some(&chars)).unwrap();
        assert_eq!(charset, DIGITS);
    }

    #[test]
    fn test_build_charset_ascii_letters() {
        let chars = vec!["ascii_letters".to_string()];
        let charset = build_charset(Some(&chars)).unwrap();
        assert_eq!(charset, format!("{}{}", ASCII_LOWERCASE, ASCII_UPPERCASE));
    }

    #[test]
    fn test_build_charset_custom() {
        let chars = vec!["xyz123".to_string()];
        let charset = build_charset(Some(&chars)).unwrap();
        assert_eq!(charset, "xyz123");
    }

    #[test]
    fn test_build_charset_empty() {
        let chars = vec!["".to_string()];
        let result = build_charset(Some(&chars));
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_seeded_password_consistent() {
        let charset = "abc123";
        let password1 = generate_seeded_password(charset, 10, "test-seed").unwrap();
        let password2 = generate_seeded_password(charset, 10, "test-seed").unwrap();
        assert_eq!(password1, password2);
        assert_eq!(password1.len(), 10);
    }

    #[test]
    fn test_generate_seeded_password_different_seeds() {
        let charset = "abc123";
        let password1 = generate_seeded_password(charset, 10, "seed1").unwrap();
        let password2 = generate_seeded_password(charset, 10, "seed2").unwrap();
        assert_ne!(password1, password2);
    }

    #[test]
    fn test_generate_random_password_length() {
        let charset = "abcdefghijklmnopqrstuvwxyz";
        let password = generate_random_password(charset, 15).unwrap();
        assert_eq!(password.len(), 15);
        assert!(password.chars().all(|c| charset.contains(c)));
    }

    #[test]
    fn test_generate_random_password_charset() {
        let charset = "ABC";
        let password = generate_random_password(charset, 100).unwrap();
        assert!(password.chars().all(|c| "ABC".contains(c)));
    }
}
