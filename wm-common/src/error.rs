use std::error::Error;
use std::fmt;

use ferrisetw::parser::ParserError;
use windows::core;

pub struct RuntimeError {
    _message: String,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self._message)
    }
}

impl fmt::Debug for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self._message)
    }
}

impl Error for RuntimeError {}
impl RuntimeError {
    pub fn new<S>(message: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            _message: message.into(),
        }
    }
}

impl From<ParserError> for RuntimeError {
    fn from(error: ParserError) -> Self {
        Self::new(format!("Parser error: {error:?}"))
    }
}

impl From<WindowsError> for RuntimeError {
    fn from(error: WindowsError) -> Self {
        Self::new(error._message)
    }
}

impl From<core::Error> for RuntimeError {
    fn from(error: core::Error) -> Self {
        Self::new(error.message())
    }
}

pub struct WindowsError {
    _code: core::HRESULT,
    _message: String,
}

impl fmt::Display for WindowsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self._message)
    }
}

impl fmt::Debug for WindowsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self._message)
    }
}

impl Error for WindowsError {}
impl WindowsError {
    pub fn new(error: core::Error) -> Self {
        Self {
            _code: error.code(),
            _message: error.message(),
        }
    }
}

impl From<core::Error> for WindowsError {
    fn from(error: core::Error) -> Self {
        Self::new(error)
    }
}
