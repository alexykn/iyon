use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiSurfaceError {
    Configuration {
        message: String,
        path: Option<String>,
    },
    Package {
        package: String,
        message: String,
    },
    Source {
        path: String,
        item: Option<String>,
        message: String,
    },
    NotConfigured {
        command: String,
    },
}

impl ApiSurfaceError {
    pub fn configuration(message: impl Into<String>, path: Option<impl Into<String>>) -> Self {
        Self::Configuration {
            message: message.into(),
            path: path.map(Into::into),
        }
    }

    pub fn package(package: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Package {
            package: package.into(),
            message: message.into(),
        }
    }

    pub fn not_configured(command: impl Into<String>) -> Self {
        Self::NotConfigured {
            command: command.into(),
        }
    }
}

impl Display for ApiSurfaceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration { message, path } => {
                write!(formatter, "api-surface configuration error")?;
                if let Some(path) = path {
                    write!(formatter, " at {path}")?;
                }
                write!(formatter, ": {message}")
            }
            Self::Package { package, message } => {
                write!(
                    formatter,
                    "api-surface package error for {package}: {message}"
                )
            }
            Self::Source {
                path,
                item,
                message,
            } => {
                write!(formatter, "api-surface source error at {path}")?;
                if let Some(item) = item {
                    write!(formatter, " ({item})")?;
                }
                write!(formatter, ": {message}")
            }
            Self::NotConfigured { command } => write!(
                formatter,
                "api-surface {command} is not configured yet; provide a repository scan configuration"
            ),
        }
    }
}

impl std::error::Error for ApiSurfaceError {}
