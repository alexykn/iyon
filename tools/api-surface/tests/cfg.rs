use std::collections::BTreeSet;

use api_surface::cfg::{CfgContext, availability};
use api_surface::model::{CrateId, ScanProfile, TargetId};
use syn::parse_quote;

fn profile() -> ScanProfile {
    ScanProfile {
        package: CrateId("fixture".into()),
        target: TargetId("fixture".into()),
        selected_features: BTreeSet::from(["enabled".into()]),
        use_default_features: true,
        target_triple: "aarch64-apple-darwin".into(),
        cfg: BTreeSet::from(["custom".into()]),
    }
}

#[test]
fn evaluates_nested_cfg_expression() {
    let attributes = vec![
        parse_quote!(#[cfg(all(feature = "enabled", any(unix, feature = "other"), not(feature = "disabled")))]),
    ];
    let result = availability(&attributes, &CfgContext::from_profile(&profile())).unwrap();
    assert!(result.active);
}

#[test]
fn unknown_cfg_is_diagnostic_error() {
    let attributes = vec![parse_quote!(#[cfg(unknown_key)])];
    assert!(availability(&attributes, &CfgContext::from_profile(&profile())).is_err());
}
