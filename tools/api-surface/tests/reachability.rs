use std::collections::BTreeSet;
use std::path::PathBuf;

use api_surface::metadata::CargoMetadataLoader;
use api_surface::parse::SourceLoader;
use api_surface::reachability::resolve;

#[test]
fn preserves_reexports_and_excludes_private_only_items() {
    let manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reachability/Cargo.toml");
    let (target, mut profile) = CargoMetadataLoader::new(manifest)
        .resolve_target("reachability", None)
        .unwrap();
    profile.selected_features = BTreeSet::new();
    let tree = SourceLoader::new(&profile)
        .load(target.source_root)
        .unwrap();
    let surface = resolve("reachability", &tree.root).unwrap();
    let paths = surface
        .paths
        .iter()
        .map(|path| path.path.display())
        .collect::<BTreeSet<_>>();
    assert!(paths.contains("reachability::Renamed"));
    assert!(paths.contains("reachability::PublicThing"));
    assert!(paths.contains("reachability::Root::field"));
    assert!(paths.contains("reachability::Root::new"));
    assert!(!paths.contains("reachability::private_only"));
    assert!(surface.items.iter().any(|item| {
        item.paths
            .iter()
            .any(|path| path.path.display() == "reachability::Renamed")
    }));
}

#[test]
fn scans_current_library_targets() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    for (package, expected) in [
        ("iyon-api", "iyon-api::ModelApi"),
        ("iyon-core", "iyon-core::ids::SessionId"),
        ("iyon-tui", "iyon-tui::View"),
        ("iyon", "iyon::tui::build_app"),
    ] {
        let (target, profile) = CargoMetadataLoader::new(&manifest)
            .resolve_target(package, None)
            .unwrap();
        let tree = SourceLoader::new(&profile)
            .load(target.source_root)
            .unwrap();
        let surface = resolve(package, &tree.root).unwrap();
        assert!(
            surface
                .paths
                .iter()
                .any(|path| path.path.display() == expected),
            "missing {expected}"
        );
    }
}
