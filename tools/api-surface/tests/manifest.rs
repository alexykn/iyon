use std::path::PathBuf;

use api_surface::check::compare_manifests;
use api_surface::metadata::CargoMetadataLoader;
use api_surface::model::SurfaceConfig;
use api_surface::parse::SourceLoader;
use api_surface::reachability::resolve;
use api_surface::render::build_manifest;
use api_surface::scan_config;

#[test]
fn manifest_is_deterministic_and_round_trips() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_manifest = root.join("tests/fixtures/manifest/Cargo.toml");
    let loader = CargoMetadataLoader::new(&workspace_manifest);
    let (target, profile) = loader.resolve_target("manifest", None).unwrap();
    let tree = SourceLoader::new(&profile)
        .load(&target.source_root)
        .unwrap();
    let surface = resolve("manifest", &tree.root).unwrap();
    let config: SurfaceConfig = toml::from_str(&format!(
        "workspace_manifest = \"{}\"\npackages = []\nmapping_dir = \"maps\"\nsdk_output_dir = \"sdk\"\noutput_dir = \"out\"",
        workspace_manifest.display()
    ))
    .unwrap();
    let package = api_surface::model::ManifestCrate {
        package: target.package,
        target: target.target,
        source_root: target.source_root,
        profile,
        surface,
    };
    let first = build_manifest(
        &config,
        workspace_manifest.parent().unwrap(),
        vec![package.clone()],
    )
    .unwrap();
    let second =
        build_manifest(&config, workspace_manifest.parent().unwrap(), vec![package]).unwrap();
    assert_eq!(first, second);
    compare_manifests(&first, &second).unwrap();
    let json = serde_json::to_string(&first).unwrap();
    assert!(!json.contains(&root.display().to_string()));
}

#[test]
fn pinned_fixture_scans_are_portable_and_hash_stable() {
    let config_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/manifest/surface.toml");
    let (_, first) = scan_config(&config_path).unwrap();
    let (_, second) = scan_config(&config_path).unwrap();

    assert_eq!(first.content_hash, second.content_hash);
    let json = serde_json::to_string(&first).unwrap();
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/home/"));
    assert!(!json.contains("aarch64-apple-darwin"));
    assert!(json.contains("x86_64-unknown-linux-gnu"));
    assert!(json.contains("src/lib.rs"));
}
