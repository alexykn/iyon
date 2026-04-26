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

            let parsed = match call.final_arguments.clone() {
                Some(arguments) => arguments,
                None => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_valid_call() {
        let mut assembler = ToolCallAssembler::default();
        assembler.start("call-1".into(), "read".into()).unwrap();
        assembler
            .push_arguments_delta("call-1", r#"{"path":"file.txt"}"#)
            .unwrap();
        assembler.finish("call-1", None).unwrap();

        let calls = assembler.finish_all().unwrap();

        assert!(matches!(
            &calls[..],
            [ToolCallRequest::Ready(call)] if call.id.0 == "call-1" && call.name == "read" && call.arguments["path"] == "file.txt"
        ));
    }

    #[test]
    fn empty_arguments_become_object() {
        let mut assembler = ToolCallAssembler::default();
        assembler.start("call-1".into(), "noop".into()).unwrap();
        assembler.finish("call-1", None).unwrap();

        let calls = assembler.finish_all().unwrap();

        assert!(matches!(
            &calls[..],
            [ToolCallRequest::Ready(call)] if call.arguments == json!({})
        ));
    }

    #[test]
    fn invalid_json_becomes_invalid_call() {
        let mut assembler = ToolCallAssembler::default();
        assembler.start("call-1".into(), "read".into()).unwrap();
        assembler.push_arguments_delta("call-1", "{").unwrap();
        assembler.finish("call-1", None).unwrap();

        let calls = assembler.finish_all().unwrap();

        assert!(matches!(
            &calls[..],
            [ToolCallRequest::Invalid(call)] if call.id.0 == "call-1" && call.name == "read"
        ));
    }

    #[test]
    fn preserves_order() {
        let mut assembler = ToolCallAssembler::default();
        assembler.start("b".into(), "second".into()).unwrap();
        assembler.start("a".into(), "first".into()).unwrap();
        assembler.finish("b", Some(json!({}))).unwrap();
        assembler.finish("a", Some(json!({}))).unwrap();

        let calls = assembler.finish_all().unwrap();

        assert!(matches!(&calls[0], ToolCallRequest::Ready(call) if call.id.0 == "b"));
        assert!(matches!(&calls[1], ToolCallRequest::Ready(call) if call.id.0 == "a"));
    }

    #[test]
    fn duplicate_start_errors() {
        let mut assembler = ToolCallAssembler::default();
        assembler.start("call-1".into(), "read".into()).unwrap();

        assert!(assembler.start("call-1".into(), "read".into()).is_err());
    }

    #[test]
    fn unknown_delta_errors() {
        let mut assembler = ToolCallAssembler::default();

        assert!(assembler.push_arguments_delta("missing", "{}").is_err());
    }
}
