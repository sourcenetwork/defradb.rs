//! Requests for owed changes, sent through the worker's journal in the steps
//! the drainer stores between (`proofs/tla/AcpOutbox.tla`, `Journaled`).

use alloy_primitives::{Bytes, B256};
use alloy_sol_types::SolCall as _;
use regolith::sync::OwnedMutexGuard;
use vera_domain::NativeTx;

use super::submission::Note;
use super::{policy_cmd, IAcp, NativeWorker, ProviderError, VeraRsProvider, ACP_ADDRESS};
use crate::provider::{owed_request_digest, OwedCall, OwedRequestOutcome};

fn unavailable(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(format!("owed request: {cause}"))
}

fn rejected(hash: B256) -> OwedRequestOutcome {
    OwedRequestOutcome::Rejected(format!("submission {hash:#x} was rejected"))
}

impl VeraRsProvider {
    /// One command alone, or several in one `batchCalls`, which the ACP
    /// precompile applies together or rolls back whole.
    pub(super) fn prepare_owed_request(
        &self,
        calls: &[OwedCall<'_>],
    ) -> Result<Vec<u8>, ProviderError> {
        let mut encoded = Vec::with_capacity(calls.len());
        for call in calls {
            let typed = policy_cmd::typed(&call.command)?;
            encoded.push(policy_cmd::calldata(
                call.bearer_token,
                call.command.policy_id,
                &typed,
            )?);
        }
        match encoded.len() {
            0 => Err(ProviderError::Config(
                "an owed request carries no command".into(),
            )),
            1 => Ok(encoded.remove(0).to_vec()),
            _ => Ok(IAcp::batchCallsCall { calls: encoded }.abi_encode()),
        }
    }

    async fn locked_worker(&self) -> Result<OwnedMutexGuard<NativeWorker>, ProviderError> {
        nuclei::time::timeout(self.sync_timeout, self.worker.clone().lock_owned())
            .await
            .map_err(|_| unavailable("worker is busy"))
    }

    /// The worker's next sequence: a request journaled from now on is signed
    /// with it or a later one.
    pub(super) async fn owed_floor(&self) -> Result<u64, ProviderError> {
        Ok(self.locked_worker().await?.next_sequence())
    }

    /// Journal `request` and return its transaction hash. A different
    /// request the journal holds is resolved first, and its outcome noted:
    /// its caller may know it by its digest alone.
    pub(super) async fn journal_owed(&self, request: &[u8]) -> Result<String, ProviderError> {
        let mut worker = self.locked_worker().await?;
        let data = Bytes::copy_from_slice(request);
        if let Some(wire) = worker.pending().map(<[u8]>::to_vec) {
            let pending = NativeTx::decode_wire(&wire).map_err(unavailable)?;
            if pending.target == ACP_ADDRESS && pending.calldata == data {
                return Ok(format!("{:#x}", pending.tx_id().0));
            }
            let (recovered, _) = self.resolve_pending(worker, wire, Note::Keep).await?;
            worker = recovered;
        }
        let wire = self
            .edge
            .spawn_blocking(move || {
                let wire = worker.prepare(ACP_ADDRESS, data)?.to_vec();
                Ok::<_, ProviderError>(wire)
            })
            .await
            .map_err(unavailable)??;
        let tx = NativeTx::decode_wire(&wire).map_err(unavailable)?;
        Ok(format!("{:#x}", tx.tx_id().0))
    }

    /// What became of the request named by `digest` and, once journaled,
    /// by `tx`. Still journaled, it is sent and waited for; its outcome is
    /// noted when its caller knows it by the digest alone. Out of the
    /// journal, a verified receipt found by `tx` decides, then the note.
    pub(super) async fn owed_outcome(
        &self,
        digest: &str,
        tx: Option<&str>,
        floor: Option<u64>,
    ) -> Result<OwedRequestOutcome, ProviderError> {
        let hash = tx
            .map(|tx| tx.parse::<B256>().map_err(unavailable))
            .transpose()?;
        let worker = self.locked_worker().await?;
        if let Some(wire) = worker.pending().map(<[u8]>::to_vec) {
            let pending = NativeTx::decode_wire(&wire).map_err(unavailable)?;
            let ours = hash == Some(pending.tx_id().0)
                || (pending.target == ACP_ADDRESS
                    && owed_request_digest(&pending.calldata) == digest);
            if ours {
                let note = if hash.is_none() {
                    Note::Keep
                } else {
                    Note::Skip
                };
                let (_, confirmed) = self.resolve_pending(worker, wire, note).await?;
                return Ok(if confirmed.receipt.success() {
                    OwedRequestOutcome::Applied
                } else {
                    rejected(confirmed.receipt.tx_hash)
                });
            }
        }
        let noted = worker.noted(digest, floor).cloned();
        drop(worker);
        if let Some(hash) = hash {
            match self.client.receipt(hash, &self.trusted).await {
                Ok(Some(response)) => {
                    let receipt = response.verify(hash, &self.trusted).map_err(unavailable)?;
                    return Ok(if receipt.success() {
                        OwedRequestOutcome::Applied
                    } else {
                        rejected(hash)
                    });
                }
                Ok(None) => {}
                Err(error) => return Err(unavailable(error)),
            }
        }
        Ok(match noted {
            Some(noted) if noted.success => OwedRequestOutcome::Applied,
            Some(noted) => {
                OwedRequestOutcome::Rejected(format!("submission {} was rejected", noted.tx))
            }
            None => OwedRequestOutcome::Unknown,
        })
    }
}
