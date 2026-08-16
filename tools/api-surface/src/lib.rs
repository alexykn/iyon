pub mod cfg;
mod error;
pub mod metadata;
pub mod model;
pub mod parse;

pub use error::ApiSurfaceError;
pub use metadata::CargoMetadataLoader;
pub use model::{CrateId, RustTarget, ScanProfile, TargetId};
pub use parse::{ModuleNode, ModuleTree, ParseDiagnostic, SourceLoader};

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
