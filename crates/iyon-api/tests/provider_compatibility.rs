#![allow(deprecated)]

use iyon_api::{MockModelApi, ModelApi, OpenAICodexModelApi, OpenRouterModelApi};

#[test]
fn deprecated_provider_types_remain_constructible_for_rust_consumers() {
    let _mock: &dyn ModelApi = &MockModelApi;
    let _openrouter =
        OpenRouterModelApi::new("test-key", "test-model").expect("valid compatibility provider");
    let _codex = OpenAICodexModelApi::new("test-token", "test-account")
        .expect("valid compatibility provider");
}
