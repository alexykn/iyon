use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// A cloneable producer of application Actions.
///
/// Sending an Action is the only operation available through this handle. It
/// is synchronous and wakes a running application without moving application
/// State or Components across threads. When `Action: Send`, the handle is
/// `Send + Sync`; non-`Send` Actions remain valid on a local runtime thread.
pub struct AppHandle<Action> {
    sender: UnboundedSender<Action>,
}

impl<Action> Clone for AppHandle<Action> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<Action> AppHandle<Action> {
    pub(crate) fn channel() -> (Self, UnboundedReceiver<Action>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    /// Sends one Action to the application's FIFO queue.
    ///
    /// The original Action is returned when the application has already
    /// closed its ingress channel.
    pub fn send(&self, action: Action) -> Result<(), AppClosed<Action>> {
        self.sender
            .send(action)
            .map_err(|error| AppClosed { action: error.0 })
    }
}

/// Indicates that an application no longer accepts external Actions.
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
