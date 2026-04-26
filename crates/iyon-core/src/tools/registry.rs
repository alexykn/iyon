#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use anyhow::bail;
use iyon_api::ModelToolSpec;

use crate::tools::{ToolDefinition, ToolExecutor, builtin::read::ReadTool};

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn ToolExecutor>>,
    active_tool_names: BTreeSet<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn ToolExecutor>) -> anyhow::Result<()> {
        let definition = tool.definition();
        if self.tools.contains_key(&definition.name) {
            bail!("tool already registered: {}", definition.name);
        }

        self.active_tool_names.insert(definition.name.clone());
        self.tools.insert(definition.name, tool);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.get(name).cloned()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    pub fn active_definitions(&self) -> Vec<ToolDefinition> {
        self.active_tool_names
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|tool| tool.definition())
            .collect()
    }

    pub fn set_active_tools<I, S>(&mut self, names: I) -> anyhow::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut next = BTreeSet::new();
        for name in names {
            let name = name.as_ref();
            if !self.tools.contains_key(name) {
                bail!("unknown tool: {name}");
            }
            next.insert(name.to_string());
        }
        self.active_tool_names = next;
        Ok(())
    }

    pub fn active_tool_names(&self) -> Vec<String> {
        self.active_tool_names.iter().cloned().collect()
    }

    pub fn model_specs(&self) -> Vec<ModelToolSpec> {
        self.active_definitions()
            .into_iter()
            .map(|definition| ModelToolSpec {
                name: definition.name,
                description: definition.description,
                input_schema: definition.input_schema,
            })
            .collect()
    }

    pub fn register_builtin_defaults(&mut self) -> anyhow::Result<()> {
        self.register(Arc::new(ReadTool))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::tools::{ToolApprovalPolicy, ToolExecutionMode, ToolFuture, ToolSource};

    #[test]
    fn duplicate_registration_fails() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::new("same"))).unwrap();

        let result = registry.register(Arc::new(FakeTool::new("same")));

        assert!(result.is_err());
    }

    #[test]
    fn builtin_defaults_include_read() {
        let mut registry = ToolRegistry::new();

        registry.register_builtin_defaults().unwrap();

        assert!(registry.get("read").is_some());
        assert_eq!(registry.active_tool_names(), vec!["read".to_string()]);
    }

    #[test]
    fn model_specs_are_active_only_and_deterministic() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FakeTool::new("zeta"))).unwrap();
        registry.register(Arc::new(FakeTool::new("alpha"))).unwrap();
        registry.set_active_tools(["zeta", "alpha"]).unwrap();

        let specs = registry.model_specs();

        assert_eq!(specs[0].name, "alpha");
        assert_eq!(specs[1].name, "zeta");

        registry.set_active_tools(["zeta"]).unwrap();
        let specs = registry.model_specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "zeta");
    }

    #[test]
    fn set_active_tools_rejects_unknown_tool() {
        let mut registry = ToolRegistry::new();

        let result = registry.set_active_tools(["missing"]);

        assert!(result.is_err());
    }

    struct FakeTool {
        name: String,
    }

    impl FakeTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl ToolExecutor for FakeTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: self.name.clone(),
                label: self.name.clone(),
                description: format!("{} description", self.name),
                input_schema: json!({ "type": "object" }),
                execution_mode: ToolExecutionMode::Parallel,
                approval: ToolApprovalPolicy::NeverAsk,
                source: ToolSource::Builtin,
                prompt_snippet: None,
                prompt_guidelines: Vec::new(),
            }
        }

        fn execute<'a>(
            &'a self,
            _ctx: crate::tools::ToolContext,
            _input: serde_json::Value,
            _updates: crate::tools::ToolUpdateSink,
        ) -> ToolFuture<'a> {
            Box::pin(async { unreachable!("registry tests do not execute tools") })
        }
    }
}
