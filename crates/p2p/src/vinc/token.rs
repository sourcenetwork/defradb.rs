//! The engine's side of the reply bridge.
//!
//! vinc's streams are `!Send` and live on the endpoint's thread, while the
//! engine answers a request from wherever its handler runs. So the engine
//! holds a token, a `Send` id, and the endpoint holds the stream under that
//! id ([`super::reply_table::ReplyTable`]). Replying spends the token; a
//! token dropped unspent asks the endpoint to reset its stream. That ask is a
//! `try_send` and can be lost when the command queue is full; the endpoint's
//! deadline sweep resets the stream then. Modeled in
//! `proofs/tla/VincReplyBridge.tla`.

use crate::rt::mpsc;

use super::command::Command;

/// A reply owed on an inbound request's stream.
pub struct VincResponseToken {
    id: u64,
    commands: Option<mpsc::Sender<Command>>,
}

impl VincResponseToken {
    pub(super) fn new(id: u64, commands: mpsc::Sender<Command>) -> Self {
        Self {
            id,
            commands: Some(commands),
        }
    }

    /// The stream's id, for the reply that spends this token.
    pub(super) fn spend(mut self) -> (u64, mpsc::Sender<Command>) {
        let commands = self
            .commands
            .take()
            .expect("a token is spent at most once, by value");
        (self.id, commands)
    }
}

impl Drop for VincResponseToken {
    fn drop(&mut self) {
        if let Some(commands) = self.commands.take() {
            let _ = commands.try_send(Command::DropReply { id: self.id });
        }
    }
}

impl std::fmt::Debug for VincResponseToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VincResponseToken")
            .field("id", &self.id)
            .finish()
    }
}
