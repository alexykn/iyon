use std::{future::Future, pin::Pin};

use futures_core::Stream;

use crate::{ModelError, ModelRequest, ModelStreamEvent};

pub type ModelStream = Pin<Box<dyn Stream<Item = Result<ModelStreamEvent, ModelError>> + Send>>;

pub type ModelStreamFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModelStream, ModelError>> + Send + 'a>>;

pub trait ModelApi: Send + Sync {
    fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_>;
}
