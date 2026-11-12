//! What every document call shares: the collection its options select, and
//! the GraphQL request it runs, signed as the options ask.

use std::sync::Arc;

use acp::nac::NodePermission;
use schema::CollectionVersion;

use crate::collection::select::collection;
use crate::request::Request;
use crate::state::NODES;
use crate::target::{CollectionArgs, Target};
use crate::types::{CollectionOptions, DefraResult};
use crate::ERR_INVALID_NODE_HANDLE;

/// A document call, ready to run on its node or in its transaction.
pub(crate) struct DocumentCall {
    target: Target,
    selector: db::CollectionSelector,
    runner: Arc<dyn query::QueryExecutor>,
    identity: Option<identity::Did>,
    context: defra_core::RequestContext,
    /// The selected collection version, set when it is not the active one:
    /// every request the call sends then works through it, as Go runs a
    /// document call on the selected collection.
    version: regolith::sync::OnceCell<String>,
}

impl DocumentCall {
    /// Resolves the call's handles, admits the caller for `permission`, and
    /// settles how the call signs: as `opts.enable_signing` asks, the node
    /// default otherwise.
    ///
    /// # Safety
    ///
    /// Every non-null string in `opts` must be a valid null-terminated string.
    pub(crate) unsafe fn begin(
        node_ptr: usize,
        opts: &CollectionOptions,
        identity_ptr: usize,
        permission: NodePermission,
    ) -> Result<Self, DefraResult> {
        let request = Request::begin(node_ptr, identity_ptr, permission)?;
        let args = CollectionArgs::read(opts);
        let node = request.node();
        let (runner, node_did, node_signing) = NODES
            .get(node, |state| {
                (
                    state.query_runner.clone(),
                    state.identity_did(),
                    state.signing_enabled,
                )
            })
            .ok_or_else(|| DefraResult::error(ERR_INVALID_NODE_HANDLE))?;
        let signing = defra_core::signing::resolve_signing_config_with_flag(
            request.identity.as_deref(),
            node_did.as_deref(),
            args.signing.unwrap_or(node_signing),
        );
        let dac_bypass = crate::query::resolve_dac_bypass_for(node, request.identity.as_deref());
        let context = request.present(
            defra_core::RequestContext::default()
                .with_txn(request.target.txn_id())
                .with_identity(request.identity.clone())
                .with_dac_bypass(dac_bypass)
                .with_batch_session(signing.as_ref().map(|s| s.public_key_hex.clone()))
                .with_signing(signing),
        );
        Ok(Self {
            identity: request.did()?,
            target: request.target,
            selector: args.selector(),
            runner,
            context,
            version: regolith::sync::OnceCell::new(),
        })
    }

    /// The node or transaction the call runs on.
    pub(crate) fn target(&self) -> &Target {
        &self.target
    }

    /// The one collection the call's options select. When it is not the
    /// active version, the call's requests work through it from here on.
    pub(crate) async fn collection(&self) -> Result<CollectionVersion, String> {
        let collection = collection(&self.runner, &self.context, &self.selector).await?;
        if !collection.is_active {
            // Set once: a call selects one collection.
            let _ = self.version.set(collection.version_id.clone());
        }
        Ok(collection)
    }

    /// The context the call acts in: the caller's identity, its transaction,
    /// its document ACP bypass and its signer, and the selected collection
    /// version when it is not the active one.
    pub(crate) fn context(&self) -> defra_core::RequestContext {
        match self.version.get() {
            Some(version) => self
                .context
                .clone()
                .with_collection_version(Some(version.clone())),
            None => self.context.clone(),
        }
    }

    /// Runs the GraphQL document `query` on the call's node or in its
    /// transaction: the request path the core's document writes take.
    pub(crate) async fn respond(&self, query: String) -> query::QueryResponse {
        self.execute(query::QueryRequest::new(query)).await
    }

    /// Runs `request`, GraphQL text or typed mutations, as the caller, on the
    /// call's node or in its transaction.
    pub(crate) async fn execute(&self, request: query::QueryRequest) -> query::QueryResponse {
        let request = request
            .with_context(self.context())
            .with_identity(self.identity.clone());
        match self.target.txn_id() {
            None => self.runner.execute(request).await,
            Some(txn_id) => {
                let handle = query::txn::TransactionHandle::new(txn_id.to_string());
                self.runner.execute_in_txn(request, &handle).await
            }
        }
    }

    /// Runs the GraphQL document `query` and returns its `data`; a GraphQL
    /// error fails the call with its message.
    pub(crate) async fn run(&self, query: String) -> Result<serde_json::Value, String> {
        let response = self.respond(query).await;
        if let Some(error) = response.errors.first() {
            return Err(error.message.clone());
        }
        Ok(response.data.unwrap_or_default())
    }
}
