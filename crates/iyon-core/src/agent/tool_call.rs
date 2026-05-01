use std::collections::BTreeMap;

use anyhow::bail;
use serde_json::{Value, json};

use crate::ids::ToolCallId;

#[derive(Debug, Clone)]
pub(crate) struct PendingToolCall {
    pub id: ToolCallId,
    pub name: String,
    pub arguments_text: String,
    finished: bool,
    final_arguments: Option<Value>,
}

#[derive(Debug, Clone)]
pub(crate) enum ToolCallRequest {
    Ready(AssembledToolCall),
    Invalid(InvalidToolCall),
}

#[derive(Debug, Clone)]
pub(crate) struct AssembledToolCall {
    pub id: ToolCallId,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct InvalidToolCall {
    pub id: ToolCallId,
    pub name: String,
    pub error: String,
}

#[derive(Debug, Default)]
pub(crate) struct ToolCallAssembler {
    calls: BTreeMap<String, PendingToolCall>,
    order: Vec<String>,
}

impl ToolCallAssembler {
    pub fn start(&mut self, id: String, name: String) -> anyhow::Result<()> {
        if id.trim().is_empty() {
            bail!("tool call id must not be empty");
        }
        if self.calls.contains_key(&id) {
            bail!("duplicate tool call id: {id}");
        }

        self.order.push(id.clone());
        self.calls.insert(
            id.clone(),
            PendingToolCall {
                id: ToolCallId(id),
                name,
                arguments_text: String::new(),
                finished: false,
                final_arguments: None,
            },
        );
        Ok(())
    }

    pub fn push_arguments_delta(&mut self, id: &str, delta: &str) -> anyhow::Result<()> {
        let Some(call) = self.calls.get_mut(id) else {
            bail!("tool call delta for unknown id: {id}");
        };
        call.arguments_text.push_str(delta);
        Ok(())
    }

    pub fn finish(&mut self, id: &str, arguments: Option<Value>) -> anyhow::Result<()> {
        let Some(call) = self.calls.get_mut(id) else {
            bail!("tool call finish for unknown id: {id}");
        };
        call.finished = true;
        call.final_arguments = arguments;
        Ok(())
    }

    pub fn finish_all(self) -> anyhow::Result<Vec<ToolCallRequest>> {
        let mut output = Vec::with_capacity(self.order.len());
        for id in self.order {
            let Some(call) = self.calls.get(&id) else {
                bail!("missing ordered tool call: {id}");
            };
            if !call.finished {
                bail!("tool call did not finish: {id}");
            }

            let parsed = if let Some(arguments) = call.final_arguments.clone() {
                arguments
            } else {
                let text = call.arguments_text.trim();
                if text.is_empty() {
                    json!({})
                } else {
                    match serde_json::from_str(text) {
                        Ok(arguments) => arguments,
                        Err(error) => {
                            output.push(ToolCallRequest::Invalid(InvalidToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                error: error.to_string(),
                            }));
                            continue;
                        }
                    }
                }
            };

            output.push(ToolCallRequest::Ready(AssembledToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: parsed,
            }));
        }
        Ok(output)
    }
}
