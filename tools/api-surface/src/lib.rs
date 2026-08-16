pub mod binding;
pub mod cfg;
pub mod check;
mod error;
pub mod metadata;
pub mod model;
pub mod normalize;
pub mod parse;
pub mod reachability;
pub mod render;
pub mod tsgen;

pub use error::ApiSurfaceError;
pub use metadata::CargoMetadataLoader;
pub use model::{
    ApiManifest, CrateId, ReachableSurface, RustTarget, ScanProfile, SurfaceConfig, SurfacePackage,
    TargetId,
};
pub use parse::{ModuleNode, ModuleTree, ParseDiagnostic, SourceLoader};
pub use reachability::resolve;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Scan,
    Check,
}

pub fn run(command: Command) -> Result<(), ApiSurfaceError> {
    let args = std::env::args().collect::<Vec<_>>();
    let config_path = argument_value(&args, "--config")
        .ok_or_else(|| ApiSurfaceError::configuration("--config is required", None::<String>))?;
    match command {
        Command::Scan => scan_from_config(&config_path).map(|_| ()),
        Command::Check => check_from_config(&config_path, argument_value(&args, "--artifacts")),
    }
}

pub fn scan_from_config(
    path: impl AsRef<std::path::Path>,
) -> Result<model::ApiManifest, ApiSurfaceError> {
    let (config, manifest) = scan_config(path)?;
    render::write_manifest(&manifest, &config)?;
    tsgen::write_sdk(&manifest, &config.mapping_dir, &config.sdk_output_dir)?;
    Ok(manifest)
}

pub fn scan_config(
    path: impl AsRef<std::path::Path>,
) -> Result<(model::SurfaceConfig, model::ApiManifest), ApiSurfaceError> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot read scanner configuration: {error}"),
            Some(path.display().to_string()),
        )
    })?;
    let mut config: model::SurfaceConfig = toml::from_str(&contents).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("invalid scanner configuration: {error}"),
            Some(path.display().to_string()),
        )
    })?;
    let config_root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    resolve_config_paths(&mut config, config_root);
    let workspace_root = config.workspace_manifest.parent().ok_or_else(|| {
        ApiSurfaceError::configuration("workspace manifest has no parent directory", None::<String>)
    })?;
    let loader = metadata::CargoMetadataLoader::new(&config.workspace_manifest);
    let mut crates = Vec::new();
    for package in &config.packages {
        let (target, mut profile) =
            loader.resolve_target(&package.package, package.target.as_deref())?;
        profile.selected_features = package.features.clone();
        profile.use_default_features = package.use_default_features;
        profile.target_triple = package
            .target_triple
            .clone()
            .unwrap_or(profile.target_triple);
        profile.cfg = package.cfg.clone();
        let tree = parse::SourceLoader::new(&profile).load(&target.source_root)?;
        let surface = reachability::resolve(&package.package, &tree.root)?;
        crates.push(model::ManifestCrate {
            package: target.package,
            target: target.target,
            source_root: target.source_root,
            profile,
            surface,
        });
    }
    let manifest = render::build_manifest(&config, workspace_root, crates)?;
    Ok((config, manifest))
}

pub fn check_from_config(
    config_path: impl AsRef<std::path::Path>,
    artifacts: Option<std::path::PathBuf>,
) -> Result<(), ApiSurfaceError> {
    let (config, manifest) = scan_config(config_path)?;
    let artifact_path = artifacts.unwrap_or_else(|| config.output_dir.join("api-manifest.json"));
    let expected = check::read_manifest(&artifact_path)?;
    check::compare_manifests(&expected, &manifest)?;
    let mapping = check::check_mappings(&manifest, &config.mapping_dir)?;
    if !mapping.missing.is_empty() || !mapping.stale.is_empty() {
        return Err(ApiSurfaceError::configuration(
            format!(
                "mapping coverage drift: missing={}, stale={}",
                mapping.missing.len(),
                mapping.stale.len()
            ),
            Some(config.mapping_dir.display().to_string()),
        ));
    }
    let reachable = manifest
        .crates
        .iter()
        .map(|package| package.surface.paths.len())
        .sum::<usize>();
    println!(
        "reachable: {reachable}\nmapped:      {}\nmissing:     {}\nstale:       {}",
        mapping.mapped,
        mapping.missing.len(),
        mapping.stale.len()
    );
    Ok(())
}

fn resolve_config_paths(config: &mut model::SurfaceConfig, root: &std::path::Path) {
    for path in [
        &mut config.workspace_manifest,
        &mut config.mapping_dir,
        &mut config.sdk_output_dir,
        &mut config.output_dir,
    ] {
        if path.is_relative() {
            *path = root.join(&*path);
        }
    }
}

fn argument_value(args: &[String], name: &str) -> Option<std::path::PathBuf> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| std::path::PathBuf::from(&pair[1]))
}
