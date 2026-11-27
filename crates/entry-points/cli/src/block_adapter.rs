use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::BlockOperations;
use storage::corekv::Store;

/// Adapter that implements BlockOperations using database.
pub struct BlockAdapter<S: Store> {
    database: Arc<db::DB<S>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S: Store + 'static> BlockAdapter<S> {
    /// Create an Arc-wrapped adapter.
    pub fn new_arc(
        database: Arc<db::DB<S>>,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Arc<dyn BlockOperations> {
        Arc::new(Self {
            database,
            document_acp,
            context: None,
        })
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }
}

#[async_trait]
impl<S: Store + 'static> BlockOperations for BlockAdapter<S> {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn BlockOperations>> {
        Some(Arc::new(Self {
            database: Arc::clone(&self.database),
            document_acp: Arc::clone(&self.document_acp),
            context: Some(context.clone()),
        }))
    }

    async fn signed_block_bytes(
        &self,
        cid: &str,
        caller_did: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let caller_did = caller_did
            .map(|did| identity::Did::try_from(did.to_string()))
            .transpose()
            .map_err(|error| format!("invalid caller DID: {error}"))?;
        let caller_identity: acp::Identity = caller_did.into();
        db::block::verify::authorized_signed_block_bytes_as(
            &self.context(),
            &self.database,
            self.document_acp.as_ref(),
            cid,
            &caller_identity,
        )
        .await
    }

    async fn verify_signature(
        &self,
        cid: &str,
        public_key: &str,
        key_type: Option<&str>,
        caller_did: Option<&str>,
    ) -> Result<(), String> {
        let crypto_key_type = match key_type.unwrap_or("secp256k1") {
            "ed25519" => crypto::KeyType::Ed25519,
            "secp256k1" => crypto::KeyType::Secp256k1,
            "secp256r1" => crypto::KeyType::Secp256r1,
            other => return Err(format!("unsupported key type: {}", other)),
        };

        let caller_did = caller_did
            .map(|did| identity::Did::try_from(did.to_string()))
            .transpose()
            .map_err(|error| format!("invalid caller DID: {error}"))?;
        let caller_identity: acp::Identity = caller_did.into();

        db::block::verify::verify_block_signature_as(
            &self.context(),
            &self.database,
            self.document_acp.as_ref(),
            cid,
            public_key,
            crypto_key_type,
            &caller_identity,
        )
        .await
    }
}
