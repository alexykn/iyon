use tokio::sync::mpsc::{self, Receiver, Sender, error::TrySendError};

const DEFAULT_ACTION_INGRESS_CAPACITY: usize = 1024;

/// A cloneable producer of application Actions.
///
/// Sending an Action is the only operation available through this handle. It
/// is synchronous and wakes a running application without moving application
/// State or Components across threads. When `Action: Send`, the handle is
/// `Send + Sync`; non-`Send` Actions remain valid on a local runtime thread.
pub struct AppHandle<Action> {
    sender: Sender<Action>,
}

impl<Action> Clone for AppHandle<Action> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<Action> AppHandle<Action> {
    pub(crate) fn channel() -> (Self, Receiver<Action>) {
        let (sender, receiver) = mpsc::channel(DEFAULT_ACTION_INGRESS_CAPACITY);
        (Self { sender }, receiver)
    }

    /// Sends one Action to the application's bounded FIFO queue without
    /// waiting. A full queue is reported explicitly and returns the Action.
    pub fn send(&self, action: Action) -> Result<(), AppSendError<Action>> {
        self.sender.try_send(action).map_err(|error| match error {
            TrySendError::Full(action) => AppSendError::Full(action),
            TrySendError::Closed(action) => AppSendError::Closed(action),
        })
    }

    /// Sends one Action while asynchronously waiting for queue capacity.
    ///
    /// This is the backpressured path for asynchronous producers. It never
    /// blocks an executor thread synchronously.
    pub async fn send_async(&self, action: Action) -> Result<(), AppClosed<Action>> {
        self.sender
            .send(action)
            .await
            .map_err(|error| AppClosed { action: error.0 })
    }
}

/// Indicates why a nonblocking Action send did not complete.
pub enum AppSendError<Action> {
    Full(Action),
    Closed(Action),
}

impl<Action> AppSendError<Action> {
    /// Borrows the Action that could not be delivered.
    pub fn action(&self) -> &Action {
        match self {
            Self::Full(action) | Self::Closed(action) => action,
        }
    }

    /// Recovers the undelivered Action.
    pub fn into_inner(self) -> Action {
        match self {
            Self::Full(action) | Self::Closed(action) => action,
        }
    }

    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full(_))
    }
}

impl<Action: std::fmt::Debug> std::fmt::Debug for AppSendError<Action> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(action) => formatter
                .debug_tuple("AppSendError::Full")
                .field(action)
                .finish(),
            Self::Closed(action) => formatter
                .debug_tuple("AppSendError::Closed")
                .field(action)
                .finish(),
        }
    }
}

impl<Action> std::fmt::Display for AppSendError<Action> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(_) => formatter.write_str("application ingress is full"),
            Self::Closed(_) => formatter.write_str("application is closed"),
        }
    }
}

impl<Action: std::fmt::Debug + 'static> std::error::Error for AppSendError<Action> {}

/// Indicates that an asynchronous Action send reached a closed application.
pub struct AppClosed<Action> {
    action: Action,
}

impl<Action> AppClosed<Action> {
    /// Borrows the Action that could not be delivered.
    pub fn action(&self) -> &Action {
        &self.action
    }

    /// Recovers the undelivered Action.
    pub fn into_inner(self) -> Action {
        self.action
    }
}

impl<Action: std::fmt::Debug> std::fmt::Debug for AppClosed<Action> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppClosed")
            .field("action", &self.action)
            .finish()
    }
}

impl<Action> std::fmt::Display for AppClosed<Action> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("application is closed")
    }
}

impl<Action: std::fmt::Debug + 'static> std::error::Error for AppClosed<Action> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonblocking_send_reports_full_and_closed_with_the_original_action() {
        let (handle, receiver) = AppHandle::<usize>::channel();
        for action in 0..DEFAULT_ACTION_INGRESS_CAPACITY {
            handle.send(action).expect("capacity should be available");
        }
        let full = handle.send(DEFAULT_ACTION_INGRESS_CAPACITY).unwrap_err();
        assert!(full.is_full());
        assert_eq!(full.into_inner(), DEFAULT_ACTION_INGRESS_CAPACITY);

        drop(receiver);
        let closed = handle.send(7).unwrap_err();
        assert!(!closed.is_full());
        assert_eq!(closed.into_inner(), 7);
    }

    #[tokio::test]
    async fn async_send_waits_for_capacity() {
        let (handle, mut receiver) = AppHandle::<usize>::channel();
        for action in 0..DEFAULT_ACTION_INGRESS_CAPACITY {
            handle.send(action).expect("capacity should be available");
        }
        let sender = handle.clone();
        let pending = tokio::spawn(async move { sender.send_async(99).await });
        tokio::task::yield_now().await;
        assert!(!pending.is_finished());

        assert_eq!(receiver.recv().await, Some(0));
        pending.await.unwrap().expect("receiver remains open");
        assert_eq!(receiver.recv().await, Some(1));
    }
}
