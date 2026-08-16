use iyon_native::EventQueueProbe;
use serde_json::json;
use tokio::time::{Duration, timeout};

const QUEUE_TIMEOUT: Duration = Duration::from_secs(1);

#[tokio::test]
async fn event_queue_preserves_fifo_order() {
    let queue = EventQueueProbe::new();
    timeout(QUEUE_TIMEOUT, queue.send(json!({"id": 1})))
        .await
        .expect("first send should resolve")
        .expect("first send");
    timeout(QUEUE_TIMEOUT, queue.send(json!({"id": 2})))
        .await
        .expect("second send should resolve")
        .expect("second send");

    assert_eq!(
        timeout(QUEUE_TIMEOUT, queue.next_event())
            .await
            .expect("first receive should resolve")
            .expect("first receive"),
        Some(json!({"id": 1}))
    );
    assert_eq!(
        timeout(QUEUE_TIMEOUT, queue.next_event())
            .await
            .expect("second receive should resolve")
            .expect("second receive"),
        Some(json!({"id": 2}))
    );
}

#[tokio::test]
async fn event_queue_close_resolves_waiter() {
    let queue = EventQueueProbe::new();
    queue.close();
    let event = timeout(QUEUE_TIMEOUT, queue.next_event())
        .await
        .expect("closed receiver should wake")
        .expect("closed receiver should not fail");
    assert_eq!(event, None);
}
