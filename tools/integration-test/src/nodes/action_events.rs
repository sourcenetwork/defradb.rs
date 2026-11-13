//! A node's collection action events, recorded from Go's
//! `action.events.start` on so a wait can find an action that already left
//! the list.

use std::time::{Duration, Instant};

use events::Message;
use regolith::sync::Atom;
use serde_json::Value;

use crate::actions::shared::ExpectedAction;

/// A node's collection action events since recording started.
pub struct ActionEvents {
    receiver: async_channel::Receiver<Message>,
    /// Every action event received, each as its JSON.
    seen: Atom<Vec<Value>>,
}

impl ActionEvents {
    /// Record the action events `receiver` delivers.
    pub(super) fn new(receiver: async_channel::Receiver<Message>) -> Self {
        Self {
            receiver,
            seen: Atom::new(Vec::new()),
        }
    }

    /// Wait until an event for `expected` has arrived since recording began,
    /// up to `deadline`.
    pub async fn arrived(
        &self,
        expected: &ExpectedAction,
        deadline: Duration,
    ) -> Result<(), String> {
        let until = Instant::now() + deadline;
        loop {
            if expected.listed_in(&Value::Array(self.seen.load_clone())) {
                return Ok(());
            }
            let remaining = until.saturating_duration_since(Instant::now());
            let message = match workers::timeout(remaining, self.receiver.recv()).await {
                Ok(Ok(message)) => message,
                Ok(Err(_)) => return Err("the node's action events closed".into()),
                Err(_) => return Err(format!("no action event {expected:?} within {deadline:?}")),
            };
            if let Some(action) = message.as_action_execution() {
                let action = serde_json::to_value(action).map_err(|e| e.to_string())?;
                self.seen.rcu(|seen| {
                    let mut next = seen.clone();
                    next.push(action.clone());
                    next
                });
            }
        }
    }
}
