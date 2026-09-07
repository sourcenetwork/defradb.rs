use alloy_primitives::Bytes;
use hub_domain::{ExecutionReceipt, NativeTx};
use tokio::sync::OwnedMutexGuard;

use super::{HubRsProvider, NativeWorker, ProviderError, ACP_ADDRESS};

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
        Err(ProviderError::Transaction(format!(
            "submission {:#x} was rejected at revision {}",
            result.receipt.tx_hash, result.revision,
        )))
    }
}

impl HubRsProvider {
    pub(super) async fn recover_pending(&self) -> Result<(), ProviderError> {
        let worker = self.worker.clone().lock_owned().await;
        if let Some(wire) = worker.pending().map(<[u8]>::to_vec) {
            let (_, confirmed) = self.resolve_pending(worker, wire).await?;
            tracing::info!(submission = %confirmed.receipt.tx_hash, success = confirmed.receipt.success(), "Recovered native submission");
        }
        Ok(())
    }

    pub(super) async fn send_tx(&self, data: Bytes) -> Result<ExecutionReceipt, ProviderError> {
        Ok(self.send_tx_with_sequence(data).await?.receipt)
    }

    pub(super) async fn send_tx_with_sequence(
        &self,
        data: Bytes,
    ) -> Result<ConfirmedSubmission, ProviderError> {
        let mut worker = tokio::time::timeout(self.sync_timeout, self.worker.clone().lock_owned())
            .await
            .map_err(|_| unavailable("worker is busy"))?;
        if let Some(wire) = worker.pending().map(<[u8]>::to_vec) {
            let tx = NativeTx::decode_wire(&wire).map_err(unavailable)?;
            let retry = tx.target == ACP_ADDRESS && tx.calldata == data;
            let (recovered, confirmed) = self.resolve_pending(worker, wire).await?;
            worker = recovered;
            if retry {
                return successful(confirmed);
            }
        }
        let (worker, wire) = tokio::task::spawn_blocking(move || {
            let wire = worker.prepare(ACP_ADDRESS, data)?.to_vec();
            Ok::<_, ProviderError>((worker, wire))
        })
        .await
        .map_err(unavailable)??;
        let (_, confirmed) = self.resolve_pending(worker, wire).await?;
        successful(confirmed)
    }

    async fn resolve_pending(
        &self,
        worker: OwnedMutexGuard<NativeWorker>,
        wire: Vec<u8>,
    ) -> Result<(OwnedMutexGuard<NativeWorker>, ConfirmedSubmission), ProviderError> {
        let tx = NativeTx::decode_wire(&wire).map_err(unavailable)?;
        let hash = tx.tx_id().0;
        let response = tokio::time::timeout(self.sync_timeout, async {
            let mut awaiting_receipt = false;
            loop {
                match self.client.receipt(hash, &self.trusted).await {
                    Ok(Some(response)) => return Ok(response),
                    Ok(None) if !awaiting_receipt => match self.client.send(&wire).await {
                        Ok(_) => awaiting_receipt = true,
                        Err(error) if error.retryable() => {}
                        // Admission errors cannot disprove an earlier submission.
                        Err(super::ClientError::Rpc { .. }) => awaiting_receipt = true,
                        Err(error) => return Err(unavailable(error)),
                    },
                    Ok(None) => {}
                    Err(error) if error.retryable() => {}
                    Err(error) => return Err(unavailable(error)),
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
        .await
        .map_err(|_| {
            unavailable(format!(
                "confirmation unavailable for {hash:#x}; request retained"
            ))
        })??;
        let revision = response.revision.height;
        self.light_client
            .wait_for_height(revision, self.sync_timeout)
            .await
            .map_err(unavailable)?;
        let trusted = self.trusted;
        tokio::task::spawn_blocking(move || {
            let mut worker = worker;
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
