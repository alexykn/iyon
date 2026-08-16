use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use api_surface::{ApiSurfaceError, Command};

#[derive(Debug, Parser)]
#[command(name = "api-surface", about = "Scan the externally reachable Rust API")]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    Scan(ScanArgs),
    Check(CheckArgs),
}

#[derive(Debug, Clone, Args)]
struct ScanArgs {
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    package: Option<String>,
    #[arg(long)]
    target: Option<String>,
    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,
    #[arg(long)]
    all_features: bool,
    #[arg(long)]
    no_default_features: bool,
    #[arg(long)]
    target_triple: Option<String>,
    #[arg(long, value_delimiter = ',')]
    cfg: Vec<String>,
    #[arg(long, default_value = "target/api-surface")]
    output: PathBuf,
}

#[derive(Debug, Clone, Args)]
struct CheckArgs {
    #[command(flatten)]
    scan: ScanArgs,
    #[arg(long, default_value = "target/api-surface")]
    artifacts: PathBuf,
}

fn main() {
    if let Err(error) = try_main() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), ApiSurfaceError> {
    let cli = Cli::parse();
    match cli.command {
        CliCommand::Scan(args) => {
            validate_inputs(&args)?;
            api_surface::run(Command::Scan)
        }
        CliCommand::Check(args) => {
            validate_inputs(&args.scan)?;
            if args.artifacts.as_os_str().is_empty() {
                return Err(ApiSurfaceError::configuration(
                    "checked-in artifacts path must not be empty",
                    None::<String>,
                ));
            }
            api_surface::run(Command::Check)
        }
    }
}

fn validate_inputs(args: &ScanArgs) -> Result<(), ApiSurfaceError> {
    if let Some(config) = &args.config {
        if !config.exists() {
            return Err(ApiSurfaceError::configuration(
                "configuration file does not exist",
                Some(config.display().to_string()),
            ));
        }
    }
    if let Some(package) = &args.package {
        if package.trim().is_empty() {
            return Err(ApiSurfaceError::package(
                package,
                "package name must not be empty",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_parses() {
        Cli::command().debug_assert();
    }

    #[test]
    fn missing_config_is_a_typed_error() {
        let args = ScanArgs {
            manifest_path: PathBuf::from("Cargo.toml"),
            config: Some(PathBuf::from("does-not-exist.toml")),
            package: Some("unknown".to_owned()),
            target: None,
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            target_triple: None,
            cfg: Vec::new(),
            output: PathBuf::from("target/api-surface"),
        };

        assert!(matches!(
            validate_inputs(&args),
            Err(ApiSurfaceError::Configuration { .. })
        ));
    }
}
