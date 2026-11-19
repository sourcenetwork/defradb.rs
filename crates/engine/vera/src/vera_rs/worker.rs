use std::path::Path;

use alloy_primitives::{Address, Bytes};
use vera_domain::{ConsensusPublicKey, ExecutionReceipt, NativeTx, ReceiptResponse};

use super::follow::{Confirmation, Journal};
use crate::provider::ProviderError;

/// Where, in a worker's directory, the submissions it saw confirmed are kept.
const CONFIRMED: &str = "confirmed";

/// Native submission journal backed by DefraDB's configured keyring, with
/// the submissions it saw confirmed (`follow::journal`).
pub struct NativeWorker {
    worker: vera_client::NativeWorker,
    confirmed: Journal,
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
        let worker = vera_client::NativeWorker::open(
            directory,
            deployment,
            |name| keyring.get(name),
            |name, bytes| keyring.set(name, bytes),
        )
        .map_err(failure)?;
        let confirmed =
            Journal::open(&directory.join(CONFIRMED), worker.next_sequence()).map_err(failure)?;
        Ok(Self { worker, confirmed })
    }

    pub fn did(&self) -> &str {
        self.worker.did()
    }
    pub fn deployment_id(&self) -> u64 {
        self.worker.deployment_id()
    }
    pub fn next_sequence(&self) -> u64 {
        self.worker.next_sequence()
    }
    pub fn pending(&self) -> Option<&[u8]> {
        self.worker.pending()
    }

    pub fn prepare(&mut self, target: Address, calldata: Bytes) -> Result<&[u8], ProviderError> {
        self.worker.prepare(target, calldata).map_err(failure)
    }

    /// Advances past the pending request once `response` verifies as its
    /// receipt. The request is kept as confirmed first, so a crash in
    /// between leaves it pending, and confirming it again keeps it once.
    pub fn acknowledge(
        &mut self,
        response: &ReceiptResponse,
        trusted: &ConsensusPublicKey,
    ) -> Result<ExecutionReceipt, ProviderError> {
        let wire = self
            .pending()
            .ok_or_else(|| failure("no pending request"))?
            .to_vec();
        let submission = NativeTx::decode_wire(&wire).map_err(failure)?.tx_id().0;
        response.verify(submission, trusted).map_err(failure)?;
        self.confirmed
            .record(&Confirmation {
                sequence: self.next_sequence(),
                submission,
                height: response.revision.height,
                wire,
            })
            .map_err(failure)?;
        self.worker.acknowledge(response, trusted).map_err(failure)
    }

    /// The submissions this worker saw confirmed.
    pub(super) fn confirmed(&mut self) -> &mut Journal {
        &mut self.confirmed
    }
}

#[cfg(test)]
mod tests;
