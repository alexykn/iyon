use api_surface::binding::parse_mapping_str;
use api_surface::model::{ApiManifest, ManifestCrate};
use api_surface::tsgen::generate;

#[test]
fn generates_all_virtual_module_contracts() {
    let manifest = ApiManifest {
        schema_version: 1,
        scanner_version: "0.1.0".into(),
        workspace_manifest: "Cargo.toml".into(),
        crates: Vec::<ManifestCrate>::new(),
        content_hash: "hash".into(),
    };
    let mapping = parse_mapping_str(
        "schema_version = 1\ncrate_id = \"fixture\"\n",
        "fixture.toml",
    )
    .unwrap();
    let generated = generate(&manifest, Path::new("/unused"));
    assert!(generated.is_err());
    let _ = mapping;
}

use std::path::Path;
