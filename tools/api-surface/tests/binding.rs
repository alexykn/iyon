use api_surface::binding::{parse_mapping_str, validate_manifest_mappings};
use api_surface::model::ApiManifest;

fn record(strategy: &str) -> String {
    format!(
        "schema_version = 1\ncrate_id = \"fixture\"\n\n[[records]]\nitem_id = \"fixture::Value\"\npath = \"fixture::Value\"\nstrategy = \"{strategy}\"\nrust_path = \"fixture::Value\"\ntypescript_module = \"iyon:api\"\ntypescript_export = \"Value\"\nimplementation_owner = \"T4\"\nstatus = \"stub\"\nnote = \"semantic projection\"\n"
    )
}

#[test]
fn accepts_all_strategies() {
    for strategy in [
        "MirrorValue",
        "LazyValue",
        "NativeHandle",
        "NativeSync",
        "NativeAsync",
        "TraitAdapter",
        "TsFacade",
        "CompatibilityProjection",
    ] {
        parse_mapping_str(&record(strategy), "fixture.toml").unwrap();
    }
}

#[test]
fn rejects_unsupported_strategy_and_module() {
    assert!(parse_mapping_str(&record("Ignore"), "fixture.toml").is_err());
    assert!(
        parse_mapping_str(
            &record("MirrorValue").replace("iyon:api", "iyon:bad"),
            "fixture.toml"
        )
        .is_err()
    );
}

#[test]
fn rejects_duplicate_records() {
    let contents = format!(
        "{}\n[[records]]\nitem_id = \"fixture::Value\"\npath = \"fixture::Value\"\nstrategy = \"MirrorValue\"\nrust_path = \"fixture::Value\"\ntypescript_module = \"iyon:api\"\ntypescript_export = \"Value2\"\nimplementation_owner = \"T4\"\nstatus = \"stub\"\nnote = \"second\"\n",
        record("MirrorValue")
    );
    assert!(parse_mapping_str(&contents, "fixture.toml").is_err());
}

#[test]
fn semantic_projection_is_a_mapping() {
    let mapping = parse_mapping_str(&record("CompatibilityProjection"), "fixture.toml").unwrap();
    let manifest = ApiManifest {
        schema_version: 1,
        scanner_version: "0.1.0".into(),
        workspace_manifest: "Cargo.toml".into(),
        crates: Vec::new(),
        content_hash: "hash".into(),
    };
    let summary = validate_manifest_mappings(&manifest, &[mapping]).unwrap();
    assert_eq!(summary.mapped, 0);
}
