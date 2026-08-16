use iyon_native::EventQueueProbe;
use serde_json::json;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn event_queue_preserves_fifo_order() {
    let queue = EventQueueProbe::new();
    queue.send(json!({"id": 1})).await.expect("first send");
    queue.send(json!({"id": 2})).await.expect("second send");

    assert_eq!(
        queue.next_event().await.expect("first receive"),
        Some(json!({"id": 1}))
    );
    assert_eq!(
        queue.next_event().await.expect("second receive"),
        Some(json!({"id": 2}))
    );
}

#[tokio::test]
async fn event_queue_close_resolves_waiter() {
    let queue = EventQueueProbe::new();
    queue.close();
    let event = timeout(Duration::from_secs(1), queue.next_event())
        .await
        .expect("closed receiver should wake")
        .expect("closed receiver should not fail");
    assert_eq!(event, None);
}
