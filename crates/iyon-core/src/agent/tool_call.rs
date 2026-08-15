use std::collections::BTreeMap;

use anyhow::bail;
use serde_json::{Value, json};

use crate::ids::ToolCallId;

#[derive(Debug, Clone)]
pub(crate) struct PendingToolCall {
    pub id: Option<ToolCallId>,
    pub name: Option<String>,
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
    calls: BTreeMap<usize, PendingToolCall>,
}

impl ToolCallAssembler {
    pub fn start(
        &mut self,
        content_index: usize,
        id: Option<String>,
        name: Option<String>,
    ) -> anyhow::Result<bool> {
        let is_new = !self.calls.contains_key(&content_index);
        if is_new {
            self.calls.insert(
                content_index,
                PendingToolCall {
                    id: None,
                    name: None,
                    arguments_text: String::new(),
                    finished: false,
                    final_arguments: None,
                },
            );
        }

        let call = self
            .calls
            .get_mut(&content_index)
            .expect("tool call was just inserted or already existed");
        bind_optional_metadata(call, id, name);
        Ok(is_new)
    }

    pub fn push_arguments_delta(
        &mut self,
        content_index: usize,
        id: Option<String>,
        name: Option<String>,
        delta: &str,
    ) -> anyhow::Result<bool> {
        let is_new = self.start(content_index, id, name)?;
        let call = self
            .calls
            .get_mut(&content_index)
            .expect("tool call was just inserted or already existed");
        call.arguments_text.push_str(delta);
        Ok(is_new)
    }

    pub fn finish(
        &mut self,
        content_index: usize,
        id: String,
        name: String,
        arguments: Option<Value>,
    ) -> anyhow::Result<bool> {
        let is_new = self.start(content_index, Some(id.clone()), Some(name.clone()))?;
        let call = self
            .calls
            .get_mut(&content_index)
            .expect("tool call was just inserted or already existed");
        if !id.trim().is_empty() {
            call.id = Some(ToolCallId(id));
        }
        call.name = Some(name);
        call.finished = true;
        call.final_arguments = arguments;
        Ok(is_new)
    }

    pub fn finish_all(self) -> anyhow::Result<Vec<ToolCallRequest>> {
        let mut output = Vec::with_capacity(self.calls.len());
        for (content_index, call) in self.calls {
            if !call.finished {
                bail!("tool call did not finish: {content_index}");
            }

            let id = call
                .id
                .clone()
                .unwrap_or_else(|| ToolCallId(generated_tool_call_id(content_index)));
            let name = call.name.clone().unwrap_or_default();

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
                                id: id.clone(),
                                name: name.clone(),
                                error: error.to_string(),
                            }));
                            continue;
                        }
                    }
                }
            };

            output.push(ToolCallRequest::Ready(AssembledToolCall {
                id,
                name,
                arguments: parsed,
            }));
        }
        Ok(output)
    }

    pub fn identity(&self, content_index: usize) -> anyhow::Result<(String, String)> {
        let Some(call) = self.calls.get(&content_index) else {
            bail!("tool call for unknown content index: {content_index}");
        };
        Ok((
            call.id
                .as_ref()
                .map(|id| id.0.clone())
                .unwrap_or_else(|| generated_tool_call_id(content_index)),
            call.name.clone().unwrap_or_default(),
        ))
    }
}

fn bind_optional_metadata(call: &mut PendingToolCall, id: Option<String>, name: Option<String>) {
    if call.id.is_none() && id.as_ref().is_some_and(|id| !id.trim().is_empty()) {
        call.id = id.map(ToolCallId);
    }
    if call.name.is_none() && name.is_some() {
        call.name = name;
    }
}

fn generated_tool_call_id(content_index: usize) -> String {
    format!("tool_call_{content_index}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ToolCallAssembler, ToolCallRequest};

    #[test]
    fn late_end_identity_does_not_split_a_call() {
        let mut assembler = ToolCallAssembler::default();

        assert!(
            assembler
                .start(2, None, Some("search".to_string()))
                .unwrap()
        );
        assert!(
            !assembler
                .push_arguments_delta(2, None, None, "{\"query\":")
                .unwrap()
        );
        assert!(
            !assembler
                .finish(2, "call-real".to_string(), "search".to_string(), None,)
                .unwrap()
        );

        let requests = assembler.finish_all().unwrap();
        assert_eq!(requests.len(), 1);
        let ToolCallRequest::Invalid(request) = &requests[0] else {
            panic!("expected invalid request");
        };
        assert_eq!(request.id.0, "call-real");
        assert_eq!(request.name, "search");
    }

    #[test]
    fn delta_before_start_creates_one_ready_request() {
        let mut assembler = ToolCallAssembler::default();

        assert!(
            assembler
                .push_arguments_delta(
                    4,
                    Some("call-4".to_string()),
                    Some("search".to_string()),
                    "{\"query\":\"iyon\"}",
                )
                .unwrap()
        );
        assert!(
            assembler
                .finish(
                    4,
                    "call-4".to_string(),
                    "search".to_string(),
                    Some(json!({"query": "iyon"})),
                )
                .is_ok()
        );

        let requests = assembler.finish_all().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(matches!(requests[0], ToolCallRequest::Ready(_)));
    }
}
