use std::{pin::Pin, task::Poll, time::Duration};

use futures_core::Stream;
use tokio::time::{Sleep, sleep};

use crate::{
    ContentBlock, ModelApi, ModelError, ModelMessage, ModelRequest, ModelStream, ModelStreamEvent,
    ModelStreamFuture, StopReason,
};

/// Deprecated compatibility provider. The product runtime uses
/// `plugins/providers/mock` instead.
#[deprecated(note = "use the Bun @iyon/provider-mock package in the product runtime")]
#[derive(Debug, Clone, Default)]
pub struct MockModelApi;

impl ModelApi for MockModelApi {
    fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_> {
        Box::pin(async move {
            let prompt = last_user_text(&request).unwrap_or_else(|| "there".to_string());
            let response = format!("Mock response to: {prompt}");
            Ok(Box::pin(MockModelStream::new(response)) as ModelStream)
        })
    }
}

struct MockModelStream {
    response: String,
    chunks: Vec<String>,
    next_chunk: usize,
    state: MockStreamState,
    delay: Pin<Box<Sleep>>,
}

enum MockStreamState {
    InitialDelay,
    Started,
    TextStart,
    Streaming,
    TextEnd,
    Done,
    Finished,
}

impl MockModelStream {
    fn new(response: String) -> Self {
        let chunks = response
            .split_inclusive(' ')
            .map(str::to_string)
            .collect::<Vec<_>>();

        Self {
            response,
            chunks,
            next_chunk: 0,
            state: MockStreamState::InitialDelay,
            delay: Box::pin(sleep(Duration::from_secs(1))),
        }
    }

    fn reset_delay(&mut self, duration: Duration) {
        self.delay = Box::pin(sleep(duration));
    }
}

impl Stream for MockModelStream {
    type Item = Result<ModelStreamEvent, ModelError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if matches!(
            self.state,
            MockStreamState::InitialDelay | MockStreamState::Streaming
        ) && self.delay.as_mut().poll(cx).is_pending()
        {
            return Poll::Pending;
        }

        match self.state {
            MockStreamState::InitialDelay => {
                self.state = MockStreamState::Started;
                Poll::Ready(Some(Ok(ModelStreamEvent::Started)))
            }
            MockStreamState::Started => {
                self.state = MockStreamState::TextStart;
                Poll::Ready(Some(Ok(ModelStreamEvent::TextStart { content_index: 0 })))
            }
            MockStreamState::TextStart | MockStreamState::Streaming => {
                let Some(delta) = self.chunks.get(self.next_chunk).cloned() else {
                    self.state = MockStreamState::TextEnd;
                    return Poll::Ready(Some(Ok(ModelStreamEvent::TextEnd {
                        content_index: 0,
                        text: self.response.clone(),
                    })));
                };

                self.next_chunk += 1;
                self.state = MockStreamState::Streaming;
                self.reset_delay(Duration::from_millis(20));
                Poll::Ready(Some(Ok(ModelStreamEvent::TextDelta {
                    content_index: 0,
                    delta,
                })))
            }
            MockStreamState::TextEnd => {
                self.state = MockStreamState::Done;
                Poll::Ready(Some(Ok(ModelStreamEvent::Done {
                    stop_reason: StopReason::Stop,
                })))
            }
            MockStreamState::Done => {
                self.state = MockStreamState::Finished;
                Poll::Ready(None)
            }
            MockStreamState::Finished => Poll::Ready(None),
        }
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
