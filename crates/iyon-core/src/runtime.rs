use tokio::sync::mpsc;

use crate::{CoreCommand, CoreEvent, MessageDelta};
use iyon_api::{
    ContentBlock, MockModelApi, ModelApi, ModelMessage, ModelRequest, ModelStreamEvent,
};

pub async fn run(mut command_rx: mpsc::Receiver<CoreCommand>, event_tx: mpsc::Sender<CoreEvent>) {
    let model = MockModelApi;
    let mut next_turn_id = 1_u64;

    while let Some(command) = command_rx.recv().await {
        match command {
            CoreCommand::SubmitTurn { text } => {
                let turn_id = next_turn_id;
                next_turn_id = next_turn_id.saturating_add(1);
                run_turn(turn_id, text, &model, &event_tx).await;
            }
            CoreCommand::CancelActiveTurn => {
                // No concurrent active turn yet; cancellation becomes meaningful once turns are spawned.
            }
            CoreCommand::Shutdown => break,
        }
    }
}

async fn run_turn(
    turn_id: u64,
    text: String,
    model: &impl ModelApi,
    event_tx: &mpsc::Sender<CoreEvent>,
) {
    if event_tx
        .send(CoreEvent::TurnStarted { turn_id })
        .await
        .is_err()
    {
        return;
    }

    let request = ModelRequest {
        system_prompt: None,
        messages: vec![ModelMessage::User {
            content: vec![ContentBlock::Text { text }],
        }],
        tools: Vec::new(),
    };

    let mut stream = match model.stream(request).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = event_tx
                .send(CoreEvent::TurnFailed {
                    turn_id,
                    message: error.to_string(),
                })
                .await;
            return;
        }
    };

    while let Some(event) = stream.recv().await {
        match event {
            Ok(ModelStreamEvent::Started) => {}
            Ok(ModelStreamEvent::TextDelta { delta }) => {
                if event_tx
                    .send(CoreEvent::MessageDelta {
                        turn_id,
                        delta: MessageDelta::Text(delta),
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Ok(ModelStreamEvent::Done { .. }) => {
                let _ = event_tx.send(CoreEvent::TurnFinished { turn_id }).await;
                return;
            }
            Ok(ModelStreamEvent::Error { message }) => {
                let _ = event_tx
                    .send(CoreEvent::TurnFailed { turn_id, message })
                    .await;
                return;
            }
            Err(error) => {
                let _ = event_tx
                    .send(CoreEvent::TurnFailed {
                        turn_id,
                        message: error.to_string(),
                    })
                    .await;
                return;
            }
        }
    }

    let _ = event_tx
        .send(CoreEvent::TurnFailed {
            turn_id,
            message: "model stream ended unexpectedly".to_string(),
        })
        .await;
}
