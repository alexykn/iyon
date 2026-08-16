use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cargo_metadata::{Metadata, MetadataCommand, Package, Target, TargetKind};

use crate::error::ApiSurfaceError;
use crate::model::{CrateId, RustTarget, ScanProfile, TargetId};

#[derive(Debug, Clone)]
pub struct CargoMetadataLoader {
    manifest_path: PathBuf,
}

impl CargoMetadataLoader {
    pub fn new(manifest_path: impl AsRef<Path>) -> Self {
        Self {
            manifest_path: manifest_path.as_ref().to_path_buf(),
        }
    }

    pub fn load(&self) -> Result<Metadata, ApiSurfaceError> {
        MetadataCommand::new()
            .manifest_path(&self.manifest_path)
            .no_deps()
            .exec()
            .map_err(|error| {
                ApiSurfaceError::configuration(
                    format!("cargo metadata failed: {error}"),
                    Some(self.manifest_path.display().to_string()),
                )
            })
    }

    pub fn resolve_target(
        &self,
        package_name: &str,
        target_name: Option<&str>,
    ) -> Result<(RustTarget, ScanProfile), ApiSurfaceError> {
        let metadata = self.load()?;
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .ok_or_else(|| {
                ApiSurfaceError::package(
                    package_name,
                    "package is not present in the selected workspace",
                )
            })?;
        let target = library_target(package, target_name)?;
        let declared_features = package.features.keys().cloned().collect();
        let default_features: BTreeSet<String> = package
            .features
            .get("default")
            .into_iter()
            .flat_map(|features| features.iter().cloned())
            .collect();
        let dependencies = package
            .dependencies
            .iter()
            .map(|dependency| dependency.name.clone())
            .collect();
        let crate_id = CrateId(package.name.to_string());
        let target_id = TargetId(target.name.to_string());
        let rust_target = RustTarget {
            package: crate_id.clone(),
            target: target_id.clone(),
            source_root: target.src_path.clone().into_std_path_buf(),
            declared_features,
            default_features: default_features.clone(),
            dependencies,
        };
        let profile = ScanProfile {
            package: crate_id,
            target: target_id,
            selected_features: default_features,
            use_default_features: true,
            target_triple: host_target_triple(),
            cfg: BTreeSet::new(),
        };
        Ok((rust_target, profile))
    }
}

fn host_target_triple() -> String {
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        "linux" => "unknown-linux-gnu",
        other => other,
    };
    format!("{}-{os}", std::env::consts::ARCH)
}

fn library_target<'a>(
    package: &'a Package,
    target_name: Option<&str>,
) -> Result<&'a Target, ApiSurfaceError> {
    let libraries = package
        .targets
        .iter()
        .filter(|target| {
            target
                .kind
                .iter()
                .any(|kind| matches!(kind, TargetKind::Lib | TargetKind::RLib))
        })
        .filter(|target| target_name.is_none_or(|name| target.name == name))
        .collect::<Vec<_>>();
    match libraries.as_slice() {
        [target] => Ok(target),
        [] => Err(ApiSurfaceError::package(
            package.name.to_string(),
            "no matching library target was found",
        )),
        _ => Err(ApiSurfaceError::package(
            package.name.to_string(),
            "multiple matching library targets were found; select --target explicitly",
        )),
    }
}
