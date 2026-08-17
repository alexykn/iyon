use std::collections::BTreeSet;
use std::path::PathBuf;

use api_surface::check::{check_mappings, compare_manifests, read_manifest};
use api_surface::model::ApiKind;
use api_surface::render::write_manifest;
use api_surface::scan_config;
use api_surface::tsgen::write_sdk;

#[test]
fn current_surface_has_zero_mapping_drift() {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("surface.toml");
    let (config, manifest) = scan_config(&config_path).unwrap();
    let expected = read_manifest(&config.output_dir.join("api-manifest.json")).unwrap();
    compare_manifests(&expected, &manifest).unwrap();
    let mapping = check_mappings(&manifest, &config.mapping_dir).unwrap();
    assert_eq!(mapping.missing, Vec::<String>::new());
    assert_eq!(mapping.stale, Vec::<String>::new());
    let reachable = manifest
        .crates
        .iter()
        .map(|package| package.surface.paths.len())
        .sum::<usize>();
    assert_eq!(mapping.mapped, reachable);
}

#[test]
fn current_surface_contains_inventory_oracle_paths() {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("surface.toml");
    let (_, manifest) = scan_config(&config_path).unwrap();
    let paths = manifest
        .crates
        .iter()
        .flat_map(|package| package.surface.paths.iter())
        .map(|path| path.path.display())
        .collect::<BTreeSet<_>>();
    for expected in [
        "iyon-api::ModelApi",
        "iyon-core::ids::SessionId",
        "iyon-tui::View",
        "iyon::tui::build_app",
    ] {
        assert!(paths.contains(expected), "missing {expected}");
    }
}

#[test]
fn every_reachable_method_has_a_generated_ts_declaration() {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("surface.toml");
    let (config, manifest) = scan_config(&config_path).unwrap();
    let generated = ["iyon-api.d.ts", "iyon-core.d.ts", "iyon-tui.d.ts"]
        .into_iter()
        .map(|file| std::fs::read_to_string(config.sdk_output_dir.join(file)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let mut methods = 0;
    for package in &manifest.crates {
        for item in &package.surface.items {
            if !matches!(item.kind, ApiKind::Method | ApiKind::AssociatedFunction) {
                continue;
            }
            for path in &item.paths {
                methods += 1;
                let marker = format!("// {} [", path.path.display());
                assert!(
                    generated.contains(&marker),
                    "missing generated TypeScript declaration for {}",
                    path.path.display()
                );
            }
        }
    }
    assert!(methods > 0, "the public method inventory must not be empty");
}

#[test]
fn checked_in_artifacts_are_fresh() {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("surface.toml");
    let (config, manifest) = scan_config(&config_path).unwrap();
    let temporary_root =
        std::env::temp_dir().join(format!("iyon-api-surface-{}", std::process::id()));
    let temporary_output = temporary_root.join("generated");
    let temporary_sdk = temporary_root.join("sdk");
    let mut temporary_config = config.clone();
    temporary_config.output_dir = temporary_output.clone();
    write_manifest(&manifest, &temporary_config).unwrap();
    write_sdk(&manifest, &config.mapping_dir, &temporary_sdk).unwrap();
    for file in ["api-manifest.json", "coverage.json", "mapping-report.json"] {
        let expected = std::fs::read(config.output_dir.join(file)).unwrap();
        let actual = std::fs::read(temporary_output.join(file)).unwrap();
        assert_eq!(expected, actual, "stale generated {file}");
    }
    for file in [
        "iyon-api.d.ts",
        "iyon-core.d.ts",
        "iyon-tui.d.ts",
        "iyon-plugins.d.ts",
    ] {
        let expected = std::fs::read(config.sdk_output_dir.join(file)).unwrap();
        let actual = std::fs::read(temporary_sdk.join(file)).unwrap();
        assert_eq!(expected, actual, "stale generated {file}");
    }
    std::fs::remove_dir_all(temporary_root).unwrap();
}
