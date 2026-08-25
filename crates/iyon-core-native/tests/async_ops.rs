use iyon_core_native::async_sleep;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn async_sleep_resolves() {
    assert_eq!(async_sleep(0).await.expect("sleep should resolve"), "slept");
}

#[tokio::test]
async fn async_sleep_rejects_invalid_delay() {
    let error = async_sleep(u32::MAX)
        .await
        .expect_err("oversized delay should reject");
    assert_eq!(error.status, napi::Status::InvalidArg);
}

#[tokio::test]
async fn one_hundred_concurrent_futures_complete() {
    let futures = (0..100).map(|_| async_sleep(0));
    let results = futures_util::future::join_all(futures).await;
    assert!(results.into_iter().all(|result| result.is_ok()));
}

#[tokio::test]
async fn sleep_timeout_is_observable() {
    timeout(Duration::from_secs(1), async_sleep(0))
        .await
        .expect("sleep should not hang")
        .expect("sleep should resolve");
}
