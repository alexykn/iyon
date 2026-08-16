use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CrateId(pub String);

impl Display for CrateId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TargetId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ApiItemId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ApiKind {
    Module,
    TypeAlias,
    Struct,
    StructField,
    Enum,
    Variant,
    VariantField,
    Function,
    Const,
    Static,
    Trait,
    AssociatedType,
    AssociatedConst,
    Method,
    AssociatedFunction,
    Impl,
    TraitProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ApiPath {
    pub crate_id: CrateId,
    pub segments: Vec<String>,
}

impl ApiPath {
    pub fn new(crate_id: impl Into<String>, segments: impl IntoIterator<Item = String>) -> Self {
        Self {
            crate_id: CrateId(crate_id.into()),
            segments: segments.into_iter().collect(),
        }
    }

    pub fn display(&self) -> String {
        std::iter::once(self.crate_id.0.as_str())
            .chain(self.segments.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("::")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceSpan {
    pub path: PathBuf,
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Visibility {
    Private,
    Public,
    Crate,
    Super,
    InPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CfgDecision {
    pub expression: String,
    pub active: bool,
    pub unknown: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct Availability {
    pub active: bool,
    pub cfg: Vec<CfgDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScanProfile {
    pub package: CrateId,
    pub target: TargetId,
    pub selected_features: BTreeSet<String>,
    pub use_default_features: bool,
    pub target_triple: String,
    pub cfg: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RustTarget {
    pub package: CrateId,
    pub target: TargetId,
    pub source_root: PathBuf,
    pub declared_features: BTreeSet<String>,
    pub default_features: BTreeSet<String>,
    pub dependencies: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RustSignature(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReachabilityTrace {
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfacePath {
    pub path: ApiPath,
    pub alias: bool,
    pub trace: ReachabilityTrace,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceItem {
    pub id: ApiItemId,
    pub canonical_path: ApiPath,
    pub kind: ApiKind,
    pub signature: RustSignature,
    pub visibility: Visibility,
    pub source: SourceSpan,
    pub availability: Availability,
    pub paths: Vec<SurfacePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReachableSurface {
    pub crate_id: CrateId,
    pub items: Vec<SurfaceItem>,
    pub paths: Vec<SurfacePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfacePackage {
    pub package: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub features: BTreeSet<String>,
    #[serde(default = "default_true")]
    pub use_default_features: bool,
    #[serde(default)]
    pub target_triple: Option<String>,
    #[serde(default)]
    pub cfg: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceConfig {
    pub workspace_manifest: PathBuf,
    pub packages: Vec<SurfacePackage>,
    pub mapping_dir: PathBuf,
    pub sdk_output_dir: PathBuf,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestCrate {
    pub package: CrateId,
    pub target: TargetId,
    pub source_root: PathBuf,
    pub profile: ScanProfile,
    pub surface: ReachableSurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiManifest {
    pub schema_version: u32,
    pub scanner_version: String,
    pub workspace_manifest: PathBuf,
    pub crates: Vec<ManifestCrate>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageReport {
    pub schema_version: u32,
    pub reachable: usize,
    pub mapped: usize,
    pub missing: Vec<String>,
    pub stale: Vec<String>,
    pub aliases: usize,
    pub packages: Vec<String>,
    pub profiles: Vec<ScanProfile>,
}

fn default_true() -> bool {
    true
}
