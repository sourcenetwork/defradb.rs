use std::path::Path;

use alloy_primitives::{Address, Bytes};
use vera_domain::{ConsensusPublicKey, ExecutionReceipt, ReceiptResponse};

use super::outcome_note::{Noted, OutcomeNote};
use crate::provider::ProviderError;

/// Native submission journal backed by DefraDB's configured keyring, with
/// the note of requests resolved by anyone but the caller that journaled
/// them (see `outcome_note.rs`).
pub struct NativeWorker {
    journal: vera_client::NativeWorker,
    note: OutcomeNote,
}

fn failure(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(cause.to_string())
}

impl NativeWorker {
    pub fn open(
        directory: &Path,
        keyring: &dyn keyring::Keyring,
        deployment: u64,
    ) -> Result<Self, ProviderError> {
        let journal = vera_client::NativeWorker::open(
            directory,
            deployment,
            |name| keyring.get(name),
            |name, bytes| keyring.set(name, bytes),
        )
        .map_err(failure)?;
        let note = OutcomeNote::open(directory)?;
        Ok(Self { journal, note })
    }

    pub fn did(&self) -> &str {
        self.journal.did()
    }
    pub fn deployment_id(&self) -> u64 {
        self.journal.deployment_id()
    }
    pub fn next_sequence(&self) -> u64 {
        self.journal.next_sequence()
    }
    pub fn pending(&self) -> Option<&[u8]> {
        self.journal.pending()
    }

    pub fn prepare(&mut self, target: Address, calldata: Bytes) -> Result<&[u8], ProviderError> {
        self.journal.prepare(target, calldata).map_err(failure)
    }

    pub fn acknowledge(
        &mut self,
        response: &ReceiptResponse,
        trusted: &ConsensusPublicKey,
    ) -> Result<ExecutionReceipt, ProviderError> {
        self.journal.acknowledge(response, trusted).map_err(failure)
    }

    /// Note, durably, what the pending request came to, before
    /// [`acknowledge`](Self::acknowledge) lets it go. Blocking IO.
    pub fn note(&mut self, outcome: Noted) -> Result<(), ProviderError> {
        self.note.record(outcome)
    }

    /// The outcome noted for the request whose digest is `digest`, among
    /// those numbered from `floor` on.
    pub fn noted(&self, digest: &str, floor: Option<u64>) -> Option<&Noted> {
        self.note.get(digest, floor)
    }
}

#[cfg(test)]
mod tests;
