//! KMS key exchange on a vinc peer: not carried yet.
//!
//! The KMS fetches keys over raw gossip topics, and vinc has no topic pubsub
//! yet (sourcenetwork/vinc#24), so every remote fetch is refused with that
//! reason. Local keys work as on any node.

use std::sync::Arc;

use async_trait::async_trait;

pub(super) struct NoKeyExchange;

#[async_trait]
impl kms::KeyTransport for NoKeyExchange {
    fn name(&self) -> &'static str {
        "vinc-unsupported"
    }

    async fn send_request(
        &self,
        _req: kms::EncodedFetchRequest,
    ) -> kms::Result<kms::TransportReplyStream> {
        Err(kms::Error::Internal(
            "KMS key exchange is not supported on the vinc transport yet (needs vinc topic pubsub, sourcenetwork/vinc#24)"
                .to_owned(),
        ))
    }

    fn install_handler(&self, _handler: Arc<dyn kms::IncomingHandler>) {}
}
