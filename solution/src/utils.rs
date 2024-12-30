// Common utilities shared across the files
use std::io::{Error, ErrorKind};
use std::future::Future;
use std::pin::Pin;
use std::fmt;

use crate::domain::OperationSuccess;

pub type Callback = Box<
dyn FnOnce(OperationSuccess) -> Pin<Box<dyn Future<Output = ()> + Send>>
    + Send
    + Sync,
>;

#[derive(Debug)]
pub enum SerializationError {
    Io,
    InvalidHMAC,
    InvalidMagicNumber,
    InvalidMessageType,
    InvaildData,
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializationError::Io => write!(f, "IO error"),
            SerializationError::InvalidHMAC => write!(f, "Invalid HMAC signature"),
            SerializationError::InvalidMagicNumber => write!(f, "Invalid magic number"),
            SerializationError::InvalidMessageType => write!(f, "Invalid message type"),
            SerializationError::InvaildData => write!(f, "Invalid data"),
        }
    }
}

impl std::error::Error for SerializationError {}

impl From<SerializationError> for Error {
    fn from(err: SerializationError) -> Error {
        Error::new(ErrorKind::Other, err)
    }
}
