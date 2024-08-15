use minijinja::{Error as MinijinjaError, ErrorKind as MinijinjaErrorKind};

pub fn to_minijinja_error<E: std::fmt::Display>(err: E) -> MinijinjaError {
    MinijinjaError::new(MinijinjaErrorKind::InvalidOperation, err.to_string())
}
