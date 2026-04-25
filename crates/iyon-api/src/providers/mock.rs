use std::time::Duration;

use tokio::{sync::mpsc, time::sleep};

use crate::{
    ContentBlock, ModelApi, ModelMessage, ModelRequest, ModelStreamEvent, ModelStreamFuture,
    StopReason,
};

#[derive(Debug, Clone, Default)]
pub struct MockModelApi;

impl ModelApi for MockModelApi {
    fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_> {
        Box::pin(async move {
            let (tx, rx) = mpsc::channel(32);
            let prompt = last_user_text(&request).unwrap_or_else(|| "there".to_string());

            tokio::spawn(async move {
                sleep(Duration::from_secs(1)).await;
                let _ = tx.send(Ok(ModelStreamEvent::Started)).await;
                let response = format!("Mock response to: {prompt}");
                for chunk in response.split_inclusive(' ') {
                    sleep(Duration::from_millis(20)).await;
                    if tx
                        .send(Ok(ModelStreamEvent::TextDelta {
                            delta: chunk.to_string(),
                        }))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                let _ = tx
                    .send(Ok(ModelStreamEvent::Done {
                        stop_reason: StopReason::Stop,
                    }))
                    .await;
            });

            Ok(rx)
        })
    }
}

fn last_user_text(request: &ModelRequest) -> Option<String> {
    request.messages.iter().rev().find_map(|message| {
        let ModelMessage::User { content } = message else {
            return None;
        };

        let text = content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        (!text.is_empty()).then_some(text)
    })
}
