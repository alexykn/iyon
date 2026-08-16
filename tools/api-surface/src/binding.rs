use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ApiSurfaceError;
use crate::model::ApiManifest;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Strategy {
    MirrorValue,
    LazyValue,
    NativeHandle,
    NativeSync,
    NativeAsync,
    TraitAdapter,
    TsFacade,
    CompatibilityProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MappingStatus {
    Stub,
    Planned,
    Implemented,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BindingRecord {
    pub item_id: String,
    pub path: String,
    pub strategy: Strategy,
    pub rust_path: String,
    pub typescript_module: String,
    pub typescript_export: String,
    pub implementation_owner: String,
    pub status: MappingStatus,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingFile {
    pub schema_version: u32,
    pub crate_id: String,
    #[serde(default)]
    pub records: Vec<BindingRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingSummary {
    pub mapped: usize,
    pub missing: Vec<String>,
    pub stale: Vec<String>,
    pub records: Vec<BindingRecord>,
}

pub const MAPPING_SCHEMA_VERSION: u32 = 1;

pub fn parse_mapping_str(contents: &str, source: &str) -> Result<MappingFile, ApiSurfaceError> {
    let mapping: MappingFile = toml::from_str(contents).map_err(|error| {
        ApiSurfaceError::configuration(
            format!("invalid binding mapping: {error}"),
            Some(source.to_owned()),
        )
    })?;
    validate_mapping_file(&mapping, source)?;
    Ok(mapping)
}

pub fn load_mapping_dir(path: &Path) -> Result<Vec<MappingFile>, ApiSurfaceError> {
    let mut files = std::fs::read_dir(path)
        .map_err(|error| {
            ApiSurfaceError::configuration(
                format!("cannot read mapping directory: {error}"),
                Some(path.display().to_string()),
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            ApiSurfaceError::configuration(
                format!("cannot read mapping entry: {error}"),
                Some(path.display().to_string()),
            )
        })?;
    files.sort();
    files
        .into_iter()
        .filter(|file| {
            file.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .map(|file| {
            let contents = std::fs::read_to_string(&file).map_err(|error| {
                ApiSurfaceError::configuration(
                    format!("cannot read mapping file: {error}"),
                    Some(file.display().to_string()),
                )
            })?;
            parse_mapping_str(&contents, &file.display().to_string())
        })
        .collect()
}

pub fn validate_manifest_mappings(
    manifest: &ApiManifest,
    mappings: &[MappingFile],
) -> Result<MappingSummary, ApiSurfaceError> {
    let mut expected = BTreeMap::new();
    for package in &manifest.crates {
        for item in &package.surface.items {
            for path in &item.paths {
                expected.insert(path.path.display(), item.id.0.clone());
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for mapping in mappings {
        for record in &mapping.records {
            validate_record(record)?;
            let key = (record.item_id.clone(), record.path.clone());
            if !seen.insert(key) {
                return Err(ApiSurfaceError::configuration(
                    format!("duplicate mapping key {} / {}", record.item_id, record.path),
                    None::<String>,
                ));
            }
            records.push(record.clone());
        }
    }
    records.sort();
    let mut missing = Vec::new();
    for (path, item_id) in &expected {
        if !seen.contains(&(item_id.clone(), path.clone())) {
            missing.push(path.clone());
        }
    }
    let stale = seen
        .iter()
        .filter(|(item_id, path)| expected.get(path) != Some(item_id))
        .map(|(_, path)| path.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() || !stale.is_empty() {
        return Ok(MappingSummary {
            mapped: expected.len().saturating_sub(missing.len()),
            missing,
            stale,
            records,
        });
    }
    Ok(MappingSummary {
        mapped: expected.len(),
        missing,
        stale,
        records,
    })
}

fn validate_mapping_file(mapping: &MappingFile, source: &str) -> Result<(), ApiSurfaceError> {
    if mapping.schema_version != MAPPING_SCHEMA_VERSION {
        return Err(ApiSurfaceError::configuration(
            format!(
                "unsupported mapping schema version {}",
                mapping.schema_version
            ),
            Some(source.to_owned()),
        ));
    }
    let mut keys = BTreeSet::new();
    for record in &mapping.records {
        validate_record(record)?;
        if !keys.insert((record.item_id.clone(), record.path.clone())) {
            return Err(ApiSurfaceError::configuration(
                format!("duplicate mapping key {} / {}", record.item_id, record.path),
                Some(source.to_owned()),
            ));
        }
    }
    Ok(())
}

fn validate_record(record: &BindingRecord) -> Result<(), ApiSurfaceError> {
    if record.item_id.trim().is_empty() || record.path.trim().is_empty() {
        return Err(ApiSurfaceError::configuration(
            "mapping item_id and path must not be empty",
            None::<String>,
        ));
    }
    if !matches!(
        record.typescript_module.as_str(),
        "iyon:api" | "iyon:core" | "iyon:tui" | "iyon:plugins"
    ) {
        return Err(ApiSurfaceError::configuration(
            format!(
                "invalid TypeScript virtual module `{}`",
                record.typescript_module
            ),
            None::<String>,
        ));
    }
    if record.typescript_export.trim().is_empty()
        || record.typescript_export.chars().any(char::is_whitespace)
    {
        return Err(ApiSurfaceError::configuration(
            format!(
                "invalid TypeScript export path `{}`",
                record.typescript_export
            ),
            None::<String>,
        ));
    }
    if record.implementation_owner.trim().is_empty() || record.note.trim().is_empty() {
        return Err(ApiSurfaceError::configuration(
            "mapping implementation_owner and note must not be empty",
            None::<String>,
        ));
    }
    Ok(())
}
