//! The deployment as the provider reaches it: receipts through its
//! submission transport, the newest certified term through its light client.

use std::sync::Arc;
use std::time::Duration;

use acp_light_client::AcpLightClient;
use alloy_primitives::B256;
use defra_core::thread_bounds::MaybeBoxFuture;
use vera_domain::{ConsensusPublicKey, NativeTx, ReceiptResponse};

use super::{Chain, Replayed};
use crate::provider::ProviderError;
use crate::vera_rs::client::{ClientError, VeraRsClient};

/// How long a replayed request's receipt is waited for between asks.
const RECEIPT_POLL: Duration = Duration::from_millis(200);

pub(crate) struct ProviderChain {
    pub client: Arc<VeraRsClient>,
    pub light_client: Arc<AcpLightClient>,
    pub trusted: ConsensusPublicKey,
    /// How long one replay waits for its receipt.
    pub timeout: Duration,
}

fn unavailable(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(format!("checking a Vera term: {cause}"))
}

impl ProviderChain {
    /// Whether `response`'s revision is one the newest certified term
    /// admits: a block above that term's cut of an earlier term was finalized
    /// by a device the term replaced.
    pub(crate) fn admitted(&self, response: &ReceiptResponse) -> Result<bool, ProviderError> {
        let block = vera_domain::verify_finalized_block(&response.revision, &self.trusted)
            .map_err(unavailable)?;
        Ok(self
            .light_client
            .header_chain()
            .term_certificate()
            .is_none_or(|certificate| certificate.admits(block.height, block.term)))
    }

    async fn receipt(&self, submission: B256) -> Result<Option<ReceiptResponse>, ProviderError> {
        self.client
            .receipt(submission, &self.trusted)
            .await
            .map_err(unavailable)
    }

    async fn send_and_confirm(&self, wire: &[u8]) -> Result<Replayed, ProviderError> {
        let submission = NativeTx::decode_wire(wire).map_err(unavailable)?.tx_id().0;
        let mut sent = false;
        loop {
            if let Some(response) = self.receipt(submission).await? {
                if self.admitted(&response)? {
                    let receipt = response
                        .verify(submission, &self.trusted)
                        .map_err(unavailable)?;
                    return Ok(Replayed {
                        executed: receipt.success(),
                        height: response.revision.height,
                    });
                }
            }
            if !sent {
                match self.client.send(wire).await {
                    Ok(_) => sent = true,
                    Err(error) if error.retryable() => {}
                    // An admission error cannot disprove an earlier send.
                    Err(ClientError::Rpc { .. }) => sent = true,
                    Err(error) => return Err(unavailable(error)),
                }
            }
            nuclei::time::sleep(RECEIPT_POLL).await;
        }
    }
}

impl Chain for ProviderChain {
    fn holds(&self, submission: B256) -> MaybeBoxFuture<'_, Result<bool, ProviderError>> {
        Box::pin(async move {
            match self.receipt(submission).await? {
                Some(response) => self.admitted(&response),
                None => Ok(false),
            }
        })
    }

    fn replay<'a>(&'a self, wire: &'a [u8]) -> MaybeBoxFuture<'a, Result<Replayed, ProviderError>> {
        Box::pin(async move {
            nuclei::time::timeout(self.timeout, self.send_and_confirm(wire))
                .await
                .map_err(|_| unavailable("a request sent again was not confirmed in time"))?
        })
    }
}
