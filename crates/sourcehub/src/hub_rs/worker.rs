use std::path::Path;

use alloy_primitives::{Address, Bytes};
use hub_domain::{ConsensusPublicKey, ExecutionReceipt, ReceiptResponse};

use crate::provider::ProviderError;

/// Native submission journal backed by DefraDB's configured keyring.
pub struct NativeWorker(hub_client::NativeWorker);

fn failure(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(cause.to_string())
}

impl NativeWorker {
    pub fn open(
        directory: &Path,
        keyring: &dyn keyring::Keyring,
        deployment: u64,
    ) -> Result<Self, ProviderError> {
        hub_client::NativeWorker::open(
            directory,
            deployment,
            |name| keyring.get(name),
            |name, bytes| keyring.set(name, bytes),
        )
        .map(Self)
        .map_err(failure)
    }

    pub fn did(&self) -> &str {
        self.0.did()
    }
    pub fn deployment_id(&self) -> u64 {
        self.0.deployment_id()
    }
    pub fn next_sequence(&self) -> u64 {
        self.0.next_sequence()
    }
    pub fn pending(&self) -> Option<&[u8]> {
        self.0.pending()
    }

    pub fn prepare(&mut self, target: Address, calldata: Bytes) -> Result<&[u8], ProviderError> {
        self.0.prepare(target, calldata).map_err(failure)
    }

    pub fn acknowledge(
        &mut self,
        response: &ReceiptResponse,
        trusted: &ConsensusPublicKey,
    ) -> Result<ExecutionReceipt, ProviderError> {
        self.0.acknowledge(response, trusted).map_err(failure)
    }
}

#[cfg(test)]
mod tests;
