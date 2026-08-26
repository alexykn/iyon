use iyon_core_native::{
    CancellationProbe, NativeCounter, native_counter_stats, reset_native_counter_stats,
};
use tokio::time::{Duration, sleep, timeout};

const HANDLE_TIMEOUT: Duration = Duration::from_secs(1);

#[tokio::test]
async fn cancellation_probe_stops_owned_operation() {
    let probe = CancellationProbe::new();
    let running_probe = probe.clone();
    let running = tokio::spawn(async move { running_probe.run(10_000).await });
    probe.cancel();

    let error = timeout(HANDLE_TIMEOUT, running)
        .await
        .expect("operation should stop promptly")
        .expect("operation task should join")
        .expect_err("cancel should reject the operation");
    assert_eq!(error.status, napi::Status::Cancelled);
}

#[test]
fn counter_increments() {
    reset_native_counter_stats();
    let counter = NativeCounter::new();
    assert_eq!(counter.increment(), 1);
    assert_eq!(counter.increment(), 2);
    assert_eq!(counter.value(), 2);
}

#[tokio::test]
async fn counter_finalization_is_observable() {
    reset_native_counter_stats();
    {
        let _counter = NativeCounter::new();
        assert_eq!(native_counter_stats().live, 1);
    }
    timeout(HANDLE_TIMEOUT, sleep(Duration::from_millis(1)))
        .await
        .expect("counter finalization wait should resolve");
    let stats = native_counter_stats();
    assert_eq!(stats.live, 0);
    assert_eq!(stats.finalized, 1);
}
