//! Requests for owed changes, sent through the worker's journal in the steps
//! the drainer stores between (`proofs/tla/AcpOutbox.tla`, `Journaled`).

use alloy_primitives::{Bytes, B256};
use alloy_sol_types::SolCall as _;
use regolith::sync::OwnedMutexGuard;
use vera_domain::NativeTx;

use super::submission::Note;
use super::{policy_cmd, IAcp, NativeWorker, ProviderError, VeraRsProvider, ACP_ADDRESS};
use crate::provider::{owed_request_digest, OwedCall, OwedRequestOutcome};
use crate::vera_rs::receipt_wait::Start;

fn unavailable(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(format!("owed request: {cause}"))
}

fn rejected(hash: B256) -> OwedRequestOutcome {
    OwedRequestOutcome::Rejected(format!("submission {hash:#x} was rejected"))
}

/// Whether the journaled request with hash `pending` is the one the caller
/// names: by its transaction hash when the caller recorded one, and by its
/// bytes only when the caller knows nothing but the digest. A later request
/// with the same bytes (a delegation reused for the same command) has
/// another hash, and its outcome is not the caller's.
fn is_the_request(recorded: Option<B256>, pending: B256, same_bytes: bool) -> bool {
    match recorded {
        Some(recorded) => recorded == pending,
        None => same_bytes,
    }
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

    /// The worker, for the drainer's steps. Like any submission they are
    /// admitted only while no switch of the deployment's active device is
    /// unchecked and nothing it voided waits to be owed again, checked
    /// again once the worker is held: a term check holds it while it runs.
    async fn locked_worker(&self) -> Result<OwnedMutexGuard<NativeWorker>, ProviderError> {
        self.admitted().await?;
        let worker = nuclei::time::timeout(self.sync_timeout, self.worker.clone().lock_owned())
            .await
            .map_err(|_| unavailable("worker is busy"))?;
        self.follower.admit()?;
        Ok(worker)
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
            let (recovered, _) = self
                .resolve_pending(worker, wire, Note::Keep, Start::Ask)
                .await?;
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
            let same_bytes =
                pending.target == ACP_ADDRESS && owed_request_digest(&pending.calldata) == digest;
            if is_the_request(hash, pending.tx_id().0, same_bytes) {
                let note = if hash.is_none() {
                    Note::Keep
                } else {
                    Note::Skip
                };
                let (_, confirmed) = self.resolve_pending(worker, wire, note, Start::Ask).await?;
                return Ok(if confirmed.receipt.success() {
                    OwedRequestOutcome::Applied
                } else {
                    rejected(confirmed.receipt.tx_hash)
                });
            }
        }
        let recorded = hash.map(|hash| format!("{hash:#x}"));
        let noted = worker.noted(digest, recorded.as_deref(), floor).cloned();
        drop(worker);
        if let Some(hash) = hash {
            match self.client.receipt(hash, &self.trusted).await {
                // A receipt from blocks the newest term voided is no
                // outcome: the request is not on the deployment's history.
                Ok(Some(response)) if self.follower.chain.admitted(&response)? => {
                    let receipt = response.verify(hash, &self.trusted).map_err(unavailable)?;
                    return Ok(if receipt.success() {
                        OwedRequestOutcome::Applied
                    } else {
                        rejected(hash)
                    });
                }
                Ok(_) => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_hash_names_the_request_and_bytes_alone_do_not() {
        let first = B256::repeat_byte(1);
        let later = B256::repeat_byte(2);
        assert!(is_the_request(Some(first), first, true));
        assert!(!is_the_request(Some(first), later, true));
        assert!(!is_the_request(Some(first), later, false));
    }

    #[test]
    fn without_a_hash_the_bytes_name_the_request() {
        let pending = B256::repeat_byte(3);
        assert!(is_the_request(None, pending, true));
        assert!(!is_the_request(None, pending, false));
    }
}
