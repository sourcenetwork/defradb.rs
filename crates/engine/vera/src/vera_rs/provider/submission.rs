use crate::vera_rs::client::ClientError;
use alloy_primitives::Bytes;
use regolith::sync::OwnedMutexGuard;
use vera_domain::{ExecutionReceipt, NativeTx};
use vera_modules::acp::types::PolicyCmd;

use super::{policy_cmd, NativeWorker, ProviderError, VeraRsProvider, ACP_ADDRESS};
use crate::provider::owed_request_digest;
use crate::vera_rs::outcome_note::Noted;
use crate::vera_rs::receipt_wait::{until_receipt, Start};

/// Whether resolving a journaled request notes its outcome.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Note {
    /// Its caller did not journal it: note it.
    Keep,
    /// Its caller journaled it and learns the outcome itself.
    Skip,
}

pub(super) struct ConfirmedSubmission {
    pub receipt: ExecutionReceipt,
    pub revision: u64,
    pub sequence: u64,
}

fn unavailable(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(format!("native submission: {cause}"))
}

fn successful(result: ConfirmedSubmission) -> Result<ConfirmedSubmission, ProviderError> {
    if result.receipt.success() {
        Ok(result)
    } else {
        Err(rejected(&result))
    }
}

/// The error a refused submission is reported with.
pub(super) fn rejected(result: &ConfirmedSubmission) -> ProviderError {
    ProviderError::Transaction(format!(
        "submission {:#x} was rejected at revision {}",
        result.receipt.tx_hash, result.revision,
    ))
}

impl VeraRsProvider {
    pub(super) async fn recover_pending(&self) -> Result<(), ProviderError> {
        let worker = self.worker.clone().lock_owned().await;
        if let Some(wire) = worker.pending().map(<[u8]>::to_vec) {
            let (_, confirmed) = self
                .resolve_pending(worker, wire, Note::Keep, Start::Ask)
                .await?;
            tracing::info!(submission = %confirmed.receipt.tx_hash, success = confirmed.receipt.success(), "Recovered native submission");
        }
        Ok(())
    }

    /// Submit `command` under `bearer_token`.
    pub(super) async fn send_command(
        &self,
        bearer_token: &str,
        command: &crate::PolicyCommand<'_>,
    ) -> Result<(), ProviderError> {
        let typed = policy_cmd::typed(command)?;
        self.send_policy_cmd(bearer_token, command.policy_id, &typed)
            .await
    }

    /// Submit `command` on `policy_id` under `bearer_token`.
    pub(super) async fn send_policy_cmd(
        &self,
        bearer_token: &str,
        policy_id: &str,
        command: &PolicyCmd,
    ) -> Result<(), ProviderError> {
        let calldata = policy_cmd::calldata(bearer_token, policy_id, command)?;
        self.send_tx(calldata).await.map(drop)
    }

    pub(super) async fn send_tx(&self, data: Bytes) -> Result<ExecutionReceipt, ProviderError> {
        Ok(self.send_tx_with_sequence(data).await?.receipt)
    }

    pub(super) async fn send_tx_with_sequence(
        &self,
        data: Bytes,
    ) -> Result<ConfirmedSubmission, ProviderError> {
        successful(self.confirm_tx(data).await?)
    }

    /// Submits `data` and returns its verified receipt, a refusal included,
    /// for a caller that answers a refusal itself.
    pub(super) async fn confirm_tx(
        &self,
        data: Bytes,
    ) -> Result<ConfirmedSubmission, ProviderError> {
        self.admitted().await?;
        let mut worker = nuclei::time::timeout(self.sync_timeout, self.worker.clone().lock_owned())
            .await
            .map_err(|_| unavailable("worker is busy"))?;
        // A term check holds the worker while it runs: one that ran while
        // this call waited for it may have left the provider closed or with
        // changes still to owe again.
        self.follower.admit()?;
        if let Some(wire) = worker.pending().map(<[u8]>::to_vec) {
            let tx = NativeTx::decode_wire(&wire).map_err(unavailable)?;
            let retry = tx.target == ACP_ADDRESS && tx.calldata == data;
            // Noted even when its bytes are ours: the drainer may have
            // journaled them and know the request by its digest alone.
            let (recovered, confirmed) = self
                .resolve_pending(worker, wire, Note::Keep, Start::Ask)
                .await?;
            worker = recovered;
            if retry {
                return Ok(confirmed);
            }
        }
        let (worker, wire) = self
            .edge
            .spawn_blocking(move || {
                let wire = worker.prepare(ACP_ADDRESS, data)?.to_vec();
                Ok::<_, ProviderError>((worker, wire))
            })
            .await
            .map_err(unavailable)??;
        let (_, confirmed) = self
            .resolve_pending(worker, wire, Note::Skip, Start::Send)
            .await?;
        Ok(confirmed)
    }

    /// Drives the journaled request `wire` to its acknowledged receipt,
    /// asking at each newly verified height (`receipt_wait.rs`); `start`
    /// says whether it may have been sent before.
    /// With [`Note::Keep`] its outcome is noted under its digest before the
    /// journal lets it go: the caller did not journal it, and the one that
    /// did may know it by its digest alone (`outcome_note.rs`).
    /// Modeled by proofs/tla/PendingSubmission.tla, its wait by
    /// ReceiptWait.tla, and the note by AcpOutbox.tla's JRecover.
    pub(super) async fn resolve_pending(
        &self,
        worker: OwnedMutexGuard<NativeWorker>,
        wire: Vec<u8>,
        note: Note,
        start: Start,
    ) -> Result<(OwnedMutexGuard<NativeWorker>, ConfirmedSubmission), ProviderError> {
        let tx = NativeTx::decode_wire(&wire).map_err(unavailable)?;
        let hash = tx.tx_id().0;
        let (client, trusted, wire) = (&self.client, &self.trusted, &wire);
        let response = nuclei::time::timeout(
            self.sync_timeout,
            until_receipt(
                self.light_client.as_ref(),
                self.receipt_fallback,
                start,
                move || async move {
                    match client.receipt(hash, trusted).await {
                        Ok(found) => Ok(found),
                        Err(error) if error.retryable() => Ok(None),
                        Err(error) => Err(unavailable(error)),
                    }
                },
                move || async move {
                    match client.send(wire).await {
                        Ok(_) => Ok(true),
                        Err(error) if error.retryable() => Ok(false),
                        // Admission errors cannot disprove an earlier submission.
                        Err(ClientError::Rpc { .. }) => Ok(true),
                        Err(error) => Err(unavailable(error)),
                    }
                },
            ),
        )
        .await
        .map_err(|_| {
            unavailable(format!(
                "confirmation unavailable for {hash:#x}; request retained"
            ))
        })??;
        let revision = response.revision.height;
        edge::compat(
            self.light_client
                .wait_for_height(revision, self.sync_timeout),
        )
        .await
        .map_err(unavailable)?;
        let trusted = self.trusted;
        self.edge
            .spawn_blocking(move || {
                let mut worker = worker;
                if note == Note::Keep {
                    let receipt = response.verify(hash, &trusted).map_err(unavailable)?;
                    worker.note(Noted {
                        digest: owed_request_digest(&tx.calldata),
                        tx: format!("{hash:#x}"),
                        sequence: tx.nonce,
                        success: receipt.success(),
                    })?;
                }
                let receipt = worker.acknowledge(&response, &trusted)?;
                Ok((
                    worker,
                    ConfirmedSubmission {
                        receipt,
                        revision,
                        sequence: tx.nonce,
                    },
                ))
            })
            .await
            .map_err(unavailable)?
    }
}
