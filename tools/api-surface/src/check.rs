use std::path::Path;

use crate::binding::{MappingSummary, load_mapping_dir, validate_manifest_mappings};
use crate::error::ApiSurfaceError;
use crate::model::ApiManifest;

pub fn read_manifest(path: &Path) -> Result<ApiManifest, ApiSurfaceError> {
    let contents = std::fs::read_to_string(path).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("cannot read generated manifest: {error}"),
            Some(path.display().to_string()),
        )
    })?;
    let manifest: ApiManifest = serde_json::from_str(&contents).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("invalid generated manifest: {error}"),
            Some(path.display().to_string()),
        )
    })?;
    if manifest.schema_version != crate::render::SCHEMA_VERSION {
        return Err(ApiSurfaceError::configuration(
            format!(
                "unsupported manifest schema version {}",
                manifest.schema_version
            ),
            Some(path.display().to_string()),
        ));
    }
    Ok(manifest)
}

pub fn compare_manifests(
    expected: &ApiManifest,
    actual: &ApiManifest,
) -> Result<(), ApiSurfaceError> {
    if expected.schema_version != actual.schema_version {
        return Err(drift(
            "schema version",
            expected.schema_version.to_string(),
            actual.schema_version.to_string(),
        ));
    }
    if expected.content_hash != actual.content_hash {
        return Err(drift(
            "content hash",
            expected.content_hash.clone(),
            actual.content_hash.clone(),
        ));
    }
    Ok(())
}

pub fn check_mappings(
    manifest: &ApiManifest,
    mapping_dir: &Path,
) -> Result<MappingSummary, ApiSurfaceError> {
    let mappings = load_mapping_dir(mapping_dir)?;
    validate_manifest_mappings(manifest, &mappings)
}

fn drift(field: &str, expected: String, actual: String) -> ApiSurfaceError {
    ApiSurfaceError::configuration(
        format!("generated manifest drift in {field}: expected {expected}, computed {actual}"),
        None::<String>,
    )
}
