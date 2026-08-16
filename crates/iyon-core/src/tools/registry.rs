#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use anyhow::bail;
use iyon_api::ModelToolSpec;

use crate::tools::{
    FileMutationQueue, ToolDefinition, ToolExecutor,
    builtin::{
        bash::BashTool, edit::EditTool, find::FindTool, grep::GrepTool, ls::LsTool, read::ReadTool,
        write::WriteTool,
    },
};

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

    pub(crate) fn register_builtin_defaults(
        &mut self,
        mutation_queue: FileMutationQueue,
    ) -> anyhow::Result<()> {
        self.register_read_only_defaults()?;
        self.register(Arc::new(WriteTool::new(mutation_queue.clone())))?;
        self.register(Arc::new(EditTool::new(mutation_queue)))?;
        self.register(Arc::new(BashTool))?;
        Ok(())
    }

    pub(crate) fn register_read_only_defaults(&mut self) -> anyhow::Result<()> {
        self.register(Arc::new(ReadTool))?;
        self.register(Arc::new(LsTool))?;
        self.register(Arc::new(FindTool))?;
        self.register(Arc::new(GrepTool))?;
        Ok(())
    }

    pub(crate) fn compatibility_with_builtin_defaults(
        mutation_queue: FileMutationQueue,
    ) -> anyhow::Result<Self> {
        let mut registry = Self::new();
        registry.register_builtin_defaults(mutation_queue)?;
        Ok(registry)
    }
}
