use std::{future::Future, pin::Pin};

use anyhow::Result;
use tokio::sync::mpsc;

use crate::stream::ModelStreamEvent;

pub type ModelStream = mpsc::Receiver<Result<ModelStreamEvent>>;
pub type ModelStreamFuture<'a> = Pin<Box<dyn Future<Output = Result<ModelStream>> + Send + 'a>>;

pub trait ModelApi: Send + Sync {
    fn stream(&self, request: crate::ModelRequest) -> ModelStreamFuture<'_>;
}
