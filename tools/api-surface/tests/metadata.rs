use std::path::PathBuf;

use api_surface::CargoMetadataLoader;

#[test]
fn resolves_iyon_library_target() {
    let (target, profile) = CargoMetadataLoader::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"),
    )
    .resolve_target("iyon", None)
    .expect("iyon library target");
    assert_eq!(
        target
            .source_root
            .file_name()
            .and_then(|name| name.to_str()),
        Some("lib.rs")
    );
    assert_eq!(profile.package.0, "iyon");
}
