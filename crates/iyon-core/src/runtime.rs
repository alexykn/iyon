use std::sync::Arc;

use tokio::{sync::mpsc, task::JoinHandle};

use crate::{CoreCommand, CoreEvent, MessageDelta};
use iyon_api::{ContentBlock, ModelApi, ModelMessage, ModelRequest, ModelStreamEvent};

struct ActiveTurn {
    turn_id: u64,
    handle: JoinHandle<()>,
}

#[derive(Debug)]
enum RuntimeEvent {
    TurnTaskFinished { turn_id: u64 },
}

pub async fn run(
    mut command_rx: mpsc::Receiver<CoreCommand>,
    event_tx: mpsc::Sender<CoreEvent>,
    model: Arc<dyn ModelApi>,
) {
    let (runtime_event_tx, mut runtime_event_rx) = mpsc::channel(32);
    let mut next_turn_id = 1_u64;
    let mut active_turn: Option<ActiveTurn> = None;

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    cancel_active_turn(&mut active_turn, &event_tx).await;
                    break;
                };

                match command {
                    CoreCommand::SubmitTurn { text } => {
                        cancel_active_turn(&mut active_turn, &event_tx).await;

                        let turn_id = next_turn_id;
                        next_turn_id = next_turn_id.saturating_add(1);

                        let model = Arc::clone(&model);
                        let event_tx = event_tx.clone();
                        let runtime_event_tx = runtime_event_tx.clone();
                        let handle = tokio::spawn(async move {
                            run_turn(turn_id, text, model, event_tx).await;
                            let _ = runtime_event_tx
                                .send(RuntimeEvent::TurnTaskFinished { turn_id })
                                .await;
                        });
                        active_turn = Some(ActiveTurn { turn_id, handle });
                    }
                    CoreCommand::CancelActiveTurn => {
                        cancel_active_turn(&mut active_turn, &event_tx).await;
                    }
                    CoreCommand::Shutdown => {
                        cancel_active_turn(&mut active_turn, &event_tx).await;
                        break;
                    }
                }
            }
            runtime_event = runtime_event_rx.recv() => {
                let Some(RuntimeEvent::TurnTaskFinished { turn_id }) = runtime_event else {
                    continue;
                };

                if active_turn
                    .as_ref()
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    active_turn = None;
                }
            }
        }
    }
}

async fn cancel_active_turn(
    active_turn: &mut Option<ActiveTurn>,
    event_tx: &mpsc::Sender<CoreEvent>,
) {
    let Some(active) = active_turn.take() else {
        return;
    };

    active.handle.abort();
    let _ = event_tx
        .send(CoreEvent::TurnCancelled {
            turn_id: active.turn_id,
        })
        .await;
}

async fn run_turn(
    turn_id: u64,
    text: String,
    model: Arc<dyn ModelApi>,
    event_tx: mpsc::Sender<CoreEvent>,
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
