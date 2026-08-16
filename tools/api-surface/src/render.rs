use std::path::{Path, PathBuf};

use crate::error::ApiSurfaceError;
use crate::model::{ApiManifest, CoverageReport, ManifestCrate, ReachableSurface, SurfaceConfig};

pub const SCHEMA_VERSION: u32 = 1;
pub const SCANNER_VERSION: &str = "0.1.0";

pub fn build_manifest(
    config: &SurfaceConfig,
    workspace_root: &Path,
    crates: Vec<ManifestCrate>,
) -> Result<ApiManifest, ApiSurfaceError> {
    let mut crates = crates;
    for package in &mut crates {
        normalize_surface(&mut package.surface, workspace_root);
        package.source_root = relative_path(workspace_root, &package.source_root);
    }
    crates.sort_by(|left, right| left.package.cmp(&right.package));
    let workspace_manifest = relative_path(workspace_root, &config.workspace_manifest);
    let mut manifest = ApiManifest {
        schema_version: SCHEMA_VERSION,
        scanner_version: SCANNER_VERSION.to_owned(),
        workspace_manifest,
        crates,
        content_hash: String::new(),
    };
    manifest.content_hash = content_hash(&manifest)?;
    Ok(manifest)
}

pub fn write_manifest(
    manifest: &ApiManifest,
    config: &SurfaceConfig,
) -> Result<(), ApiSurfaceError> {
    let output = config.output_dir.join("api-manifest.json");
    write_json(&output, manifest)?;
    for package in &manifest.crates {
        let output = config
            .output_dir
            .join(format!("{}-declarations.json", package.package.0));
        write_json(&output, &package.surface)?;
    }
    let coverage = coverage_report(manifest);
    write_json(&config.output_dir.join("coverage.json"), &coverage)?;
    let summary = format!(
        "reachable: {}\nmapped:      {}\nmissing:     {}\nstale:       {}\n",
        coverage.reachable,
        coverage.mapped,
        coverage.missing.len(),
        coverage.stale.len()
    );
    std::fs::create_dir_all(&config.output_dir).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot create output directory: {error}"),
            Some(config.output_dir.display().to_string()),
        )
    })?;
    std::fs::write(config.output_dir.join("summary.txt"), summary).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot write summary: {error}"),
            Some(config.output_dir.display().to_string()),
        )
    })?;
    Ok(())
}

pub fn coverage_report(manifest: &ApiManifest) -> CoverageReport {
    let paths = manifest
        .crates
        .iter()
        .flat_map(|package| package.surface.paths.iter().map(|path| path.path.display()))
        .collect::<Vec<_>>();
    let aliases = manifest
        .crates
        .iter()
        .flat_map(|package| package.surface.paths.iter())
        .filter(|path| path.alias)
        .count();
    CoverageReport {
        schema_version: SCHEMA_VERSION,
        reachable: paths.len(),
        mapped: 0,
        missing: paths,
        stale: Vec::new(),
        aliases,
        packages: manifest
            .crates
            .iter()
            .map(|package| package.package.0.clone())
            .collect(),
        profiles: manifest
            .crates
            .iter()
            .map(|package| package.profile.clone())
            .collect(),
    }
}

pub fn content_hash<T: serde::Serialize>(value: &T) -> Result<String, ApiSurfaceError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot serialize manifest for hashing: {error}"),
            None::<String>,
        )
    })?;
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("{hash:016x}"))
}

pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), ApiSurfaceError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot create output directory: {error}"),
            Some(parent.display().to_string()),
        )
    })?;
    let json = serde_json::to_string_pretty(value).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot serialize generated JSON: {error}"),
            None::<String>,
        )
    })?;
    std::fs::write(path, format!("{json}\n")).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot write generated JSON: {error}"),
            Some(path.display().to_string()),
        )
    })
}

fn normalize_surface(surface: &mut ReachableSurface, workspace_root: &Path) {
    for item in &mut surface.items {
        item.source.path = relative_path(workspace_root, &item.source.path);
        for path in &mut item.paths {
            normalize_trace(&mut path.trace.steps, workspace_root);
        }
    }
    for path in &mut surface.paths {
        normalize_trace(&mut path.trace.steps, workspace_root);
    }
    surface.items.sort_by(|left, right| left.id.cmp(&right.id));
    surface
        .paths
        .sort_by(|left, right| left.path.cmp(&right.path));
}

fn normalize_trace(steps: &mut [String], workspace_root: &Path) {
    let root = workspace_root.to_string_lossy();
    for step in steps {
        if let Some(index) = step.find(root.as_ref()) {
            let suffix = step[index + root.len()..]
                .trim_start_matches(['/', '\\'])
                .to_owned();
            step.replace_range(index.., &suffix);
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .into()
}
