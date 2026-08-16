mod error;

pub use error::ApiSurfaceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Scan,
    Check,
}

pub fn run(command: Command) -> Result<(), ApiSurfaceError> {
    Err(ApiSurfaceError::not_configured(match command {
        Command::Scan => "scan",
        Command::Check => "check",
    }))
}
