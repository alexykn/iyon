use std::{collections::HashMap, future::Future, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct FileMutationQueue {
    locks: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
}

impl FileMutationQueue {
    pub async fn run<F, Fut, T>(&self, path: PathBuf, op: F) -> anyhow::Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let lock = self.lock_for_path(path).await;
        let _guard = lock.lock().await;
        op().await
    }

    async fn lock_for_path(&self, path: PathBuf) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.entry(path).or_default().clone()
    }
}
