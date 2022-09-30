use std::convert::From;
use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::result;

use nix::Error as NixError;
use serde_yaml::Error as YamlError;

/// A specialized type `rash` operations.
pub type Result<T> = result::Result<T, Error>;

/// The error type for `rash` executions.
pub struct Error {
    repr: Repr,
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.repr, f)
    }
}

enum Repr {
    Simple(ErrorKind),
    Custom(Box<Custom>),
}

#[derive(Debug)]
struct Custom {
    kind: ErrorKind,
    error: Box<dyn StdError + Send + Sync>,
}

/// A list specifying general categories of `rash` error.
///
/// This list is intended to grow over time and it is not recommended to
/// exhaustively match against it.
///
/// Use it with the [`StdError`] type.
///
/// [`StdError`]: struct.Error.html
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Program finish gracefully returning 0
    GracefulExit,
    /// An entity was not found, often a module.
    NotFound,
    /// Data is invalid, often fail to render Tera.
    InvalidData,
    /// I/O error propagation
    IOError,
    /// Omit module parameter
    OmitParam,
    /// Generic failure during subprocess execution.
    SubprocessFail,
    /// Task stack is empty
    EmptyTaskStack,
    /// Any `rash` error not part of this list.
    Other,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::GracefulExit => "program finish gracefully",
            ErrorKind::NotFound => "entity not found",
            ErrorKind::InvalidData => "invalid data",
            ErrorKind::IOError => "I/O error",
            ErrorKind::OmitParam => "omit param",
            ErrorKind::SubprocessFail => "subprocess fail",
            ErrorKind::EmptyTaskStack => "task stack is empty",
            ErrorKind::Other => "other os error",
        }
    }
}

/// Intended to use with errors that aren't exposed to the user, where allocating onto
/// the heap (for normal construction via Error::new) is too costly.
impl From<ErrorKind> for Error {
    /// Converts an [`ErrorKind`] into an [`Error`].
    ///
    /// This conversion allocates a new error with a simple representation of error kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::{Error, ErrorKind};
    ///
    /// let not_found = ErrorKind::NotFound;
    /// let error = Error::from(not_found);
    /// assert_eq!("entity not found", format!("{}", error));
    /// ```
    ///
    /// [`ErrorKind`]: enum.ErrorKind.html
    /// [`Error`]: struct.Error.html
    #[inline]
    fn from(kind: ErrorKind) -> Error {
        Error {
            repr: Repr::Simple(kind),
        }
    }
}

impl From<io::Error> for Error {
    /// Converts an [`io::Error`] into an [`Error`].
    ///
    /// This conversion allocates a new error with a ErrorKind::IOError of error kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::Error;
    /// use std::io;
    ///
    /// let io_error = io::Error::new(io::ErrorKind::NotFound, "fail");
    /// let error = Error::from(io_error);
    /// assert_eq!("fail", format!("{}", error));
    /// ```
    ///
    /// [`io::Error`]: ../../std/io/type.Error.html
    /// [`Error`]: struct.Error.html
    #[inline]
    fn from(e: io::Error) -> Error {
        Error::new(ErrorKind::IOError, e)
    }
}

impl From<YamlError> for Error {
    /// Converts an YamlError into an [`Error`].
    ///
    /// This conversion allocates a new error with a custom representation of serde_yaml error.
    ///
    ///
    /// [`Error`]: ../error/struct.Error.html
    #[inline]
    fn from(error: YamlError) -> Error {
        Error {
            repr: Repr::Custom(Box::new(Custom {
                kind: ErrorKind::InvalidData,
                error: Box::new(error),
            })),
        }
    }
}

impl From<NixError> for Error {
    /// Converts an NixError into an [`Error`].
    ///
    /// This conversion allocates a new error with a custom representation of nix error.
    ///
    ///
    /// [`Error`]: ../error/struct.Error.html
    #[inline]
    fn from(error: NixError) -> Error {
        Error {
            repr: Repr::Custom(Box::new(Custom {
                kind: ErrorKind::Other,
                error: Box::new(error),
            })),
        }
    }
}

impl Error {
    /// Creates a new `rash` error from a known kind of error as well as an
    /// arbitrary error payload.
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::{Error, ErrorKind};
    ///
    /// let invalid_data_err = Error::new(
    ///     ErrorKind::InvalidData,
    ///     "no valid data",
    /// );
    /// let custom_error = Error::new(
    ///     ErrorKind::Other,
    ///     invalid_data_err,
    /// );

    /// ```
    pub fn new<E>(kind: ErrorKind, error: E) -> Error
    where
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        Self::_new(kind, error.into())
    }

    fn _new(kind: ErrorKind, error: Box<dyn StdError + Send + Sync>) -> Error {
        Error {
            repr: Repr::Custom(Box::new(Custom { kind, error })),
        }
    }

    /// Returns the corresponding `ErrorKind` for this error.
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::{Error, ErrorKind};
    ///
    /// fn print_error(err: Error) {
    ///     println!("{:?}", err.kind());
    /// }
    ///
    /// print_error(Error::new(ErrorKind::InvalidData, "oh no!"));
    /// ```
    pub fn kind(&self) -> ErrorKind {
        match self.repr {
            Repr::Custom(ref c) => c.kind,
            Repr::Simple(kind) => kind,
        }
    }

    /// Returns the OS error that this error represents (if any).
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::{Error, ErrorKind};
    ///
    /// fn print_os_error(err: &Error) {
    ///     if let Some(raw_os_err) = err.raw_os_error() {
    ///         println!("raw OS error: {:?}", raw_os_err);
    ///     } else {
    ///         println!("Not an OS error");
    ///     }
    /// }
    ///
    /// print_os_error(&Error::new(ErrorKind::Other, "oh no!"));
    ///
    /// ```
    pub fn raw_os_error(&self) -> Option<i32> {
        match self.repr {
            Repr::Custom(..) => None,
            Repr::Simple(..) => None,
        }
    }

    /// Returns a reference to the inner error wrapped by this error (if any).
    ///
    /// If this `Error` was constructed via `new` then this function will
    /// return `Some`, otherwise it will return `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::{Error, ErrorKind};
    ///
    /// fn print_error(err: &Error) {
    ///     if let Some(inner_err) = err.get_ref() {
    ///         println!("Inner error: {:?}", inner_err);
    ///     } else {
    ///         println!("No inner error");
    ///     }
    /// }
    ///
    /// print_error(&Error::new(ErrorKind::Other, "oh no!"));
    /// ```
    pub fn get_ref(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        match self.repr {
            Repr::Simple(..) => None,
            Repr::Custom(ref c) => Some(&*c.error),
        }
    }

    /// Returns a mutable reference to the inner error wrapped by this error
    /// (if any).
    ///
    /// If this `Error` was constructed via `new` then this function will
    /// return `Some`, otherwise it will return `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::{Error, ErrorKind};
    /// use std::{error, fmt};
    /// use std::fmt::Display;
    ///
    /// #[derive(Debug)]
    /// struct MyError {
    ///     v: String,
    /// }
    ///
    /// impl MyError {
    ///     fn new() -> MyError {
    ///         MyError {
    ///             v: "oh no!".to_string()
    ///         }
    ///     }
    ///
    ///     fn change_message(&mut self, new_message: &str) {
    ///         self.v = new_message.to_string();
    ///     }
    /// }
    ///
    /// impl error::Error for MyError {}
    ///
    /// impl Display for MyError {
    ///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         write!(f, "MyError: {}", &self.v)
    ///     }
    /// }
    ///
    /// fn change_error(mut err: Error) -> Error {
    ///     if let Some(inner_err) = err.get_mut() {
    ///         inner_err.downcast_mut::<MyError>().unwrap().change_message("I've been changed!");
    ///     }
    ///     err
    /// }
    ///
    /// fn print_error(err: &Error) {
    ///     if let Some(inner_err) = err.get_ref() {
    ///         println!("Inner error: {}", inner_err);
    ///     } else {
    ///         println!("No inner error");
    ///     }
    /// }
    ///
    /// print_error(&change_error(Error::new(ErrorKind::Other, MyError::new())));
    /// ```
    pub fn get_mut(&mut self) -> Option<&mut (dyn StdError + Send + Sync + 'static)> {
        match self.repr {
            Repr::Simple(..) => None,
            Repr::Custom(ref mut c) => Some(&mut *c.error),
        }
    }

    /// Consumes the `Error`, returning its inner error (if any).
    ///
    /// If this `Error` was constructed via `new` then this function will
    /// return `Some`, otherwise it will return `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rash_core::error::{Error, ErrorKind};
    ///
    /// fn print_error(err: Error) {
    ///     if let Some(inner_err) = err.into_inner() {
    ///         println!("Inner error: {}", inner_err);
    ///     } else {
    ///         println!("No inner error");
    ///     }
    /// }
    ///
    /// print_error(Error::new(ErrorKind::Other, "oh no!"));
    /// ```
    pub fn into_inner(self) -> Option<Box<dyn StdError + Send + Sync>> {
        match self.repr {
            Repr::Simple(..) => None,
            Repr::Custom(c) => Some(c.error),
        }
    }
}

impl fmt::Debug for Repr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Repr::Custom(ref c) => fmt::Debug::fmt(&c, fmt),
            Repr::Simple(kind) => fmt.debug_tuple("Kind").field(&kind).finish(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.repr {
            Repr::Custom(ref c) => c.error.fmt(fmt),
            Repr::Simple(kind) => write!(fmt, "{}", kind.as_str()),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self.repr {
            Repr::Simple(..) => None,
            Repr::Custom(ref c) => c.error.source(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fmt;

    #[test]
    fn test_debug_error() {
        let err = Error {
            repr: Repr::Custom(Box::new(Custom {
                kind: ErrorKind::InvalidData,
                error: Box::new(Error {
                    repr: super::Repr::Custom(Box::new(Custom {
                        kind: ErrorKind::Other,
                        error: Box::new(Error::new(ErrorKind::Other, "oh no!")),
                    })),
                }),
            })),
        };
        let expected = "\
        Custom { \
            kind: InvalidData, \
            error: Custom { \
                kind: Other, \
                error: Custom { \
                    kind: Other, \
                    error: \"oh no!\" \
                } \
            } \
         }";
        assert_eq!(format!("{:?}", err), expected);
    }

    #[test]
    fn test_downcasting() {
        #[derive(Debug)]
        struct TestError;

        impl fmt::Display for TestError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("asdf")
            }
        }

        impl StdError for TestError {}

        // we have to call all of these UFCS style right now since method
        // resolution won't implicitly drop the Send+Sync bounds
        let mut err = Error::new(ErrorKind::Other, TestError);
        assert!(err.get_ref().unwrap().is::<TestError>());
        assert_eq!("asdf", err.get_ref().unwrap().to_string());
        assert!(err.get_mut().unwrap().is::<TestError>());
        let extracted = err.into_inner().unwrap();
        extracted.downcast::<TestError>().unwrap();
    }
}
