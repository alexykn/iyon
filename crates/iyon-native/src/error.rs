use napi::{Error, Status};

/// Stable error categories exposed by the smoke bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeError {
    InvalidInput,
    Internal,
    Cancelled,
    Closed,
}

impl NativeError {
    pub fn invalid_input(message: impl Into<String>) -> Error {
        Error::new(
            Status::InvalidArg,
            format!("invalid input: {}", message.into()),
        )
    }

    pub fn internal(message: impl Into<String>) -> Error {
        Error::new(
            Status::GenericFailure,
            format!("internal failure: {}", message.into()),
        )
    }

    pub fn cancelled() -> Error {
        Error::new(Status::Cancelled, "operation cancelled")
    }

    pub fn closed() -> Error {
        Error::new(Status::Closing, "native operation is closed")
    }
}
