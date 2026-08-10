use std::{error::Error, fmt};

/// Failure returned by [`super::App::run`].
#[derive(Debug)]
pub enum RunError<ApplicationError> {
    /// The application's init or update callback failed.
    Application(ApplicationError),
    /// The runtime, terminal adapter, or framework failed.
    Runtime(RuntimeError),
}

impl<ApplicationError: fmt::Display> fmt::Display for RunError<ApplicationError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Application(error) => write!(formatter, "application error: {error}"),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl<ApplicationError> From<RuntimeError> for RunError<ApplicationError> {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl<ApplicationError: Error + 'static> Error for RunError<ApplicationError> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Application(error) => Some(error),
            Self::Runtime(error) => Some(error),
        }
    }
}

/// Opaque runtime failure.
///
/// Backend-library errors and framework-internal errors are deliberately
/// mapped to this stable runtime-facing type.
pub struct RuntimeError {
    source: anyhow::Error,
}

impl RuntimeError {
    pub(crate) fn new(source: impl Into<anyhow::Error>) -> Self {
        Self {
            source: source.into(),
        }
    }

    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self::new(anyhow::anyhow!(message.into()))
    }
}

impl fmt::Debug for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeError")
            .field("source", &self.source)
            .finish()
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.source()
    }
}
