pub mod file;

use crate::error::{Error, ErrorKind, Result};

pub fn parse_octal(s: &str) -> Result<u32> {
    match s.len() {
        3 => u32::from_str_radix(&s, 8).or_else(|e| Err(Error::new(ErrorKind::InvalidData, e))),
        4 => u32::from_str_radix(s.get(1..).unwrap(), 8)
            .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e))),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{} cannot be parsed to octal", s),
        )),
    }
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
}
