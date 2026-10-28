//! Query execution with the request's context.
//!
//! The acting identity, the signing config, the batch signing session and the
//! DAC bypass travel in the request's `RequestContext`, so a query runs on the
//! task that received it, with no hop to another thread.

use std::sync::Arc;

use defra_core::signing::SigningConfig;
use edge::Edge;
use query::executor::{QueryExecutor, QueryRequest, QueryResponse};
use query::txn::TransactionHandle;

use crate::identity_extractor::ExtractIdentity;
use crate::router::AppState;

/// Execute a GraphQL query with signing config and DAC bypass context.
pub async fn execute_with_context(
    state: &AppState,
    identity: &ExtractIdentity,
    mut request: QueryRequest,
) -> QueryResponse {
    request.context = request_context(state, identity).await;
    let context = request.context.clone();
    context
        .mirror(execute_prepared_with_executor(
            state.executor.as_ref(),
            request,
        ))
        .await
}

async fn execute_prepared_with_executor(
    executor: &dyn QueryExecutor,
    request: QueryRequest,
) -> QueryResponse {
    let prepared = match executor.prepare_request(&request).await {
        Ok(prepared) => prepared,
        Err(error) => {
            return QueryResponse::error(query::executor::QueryResponseError::from_query_error(
                error,
            ))
        }
    };
    execute_prepared_request_resolving_inputs(executor, request, prepared).await
}

async fn execute_prepared_request_resolving_inputs(
    executor: &dyn QueryExecutor,
    request: QueryRequest,
    mut prepared: Option<Arc<query::prepared::PreparedMutations>>,
) -> QueryResponse {
    loop {
        let response = executor
            .execute_prepared(request.clone(), prepared.clone())
            .await;
        if let Some(input) = response.retry_input() {
            match executor
                .resolve_retry_input(&request, prepared.clone(), input)
                .await
            {
                Ok(Some(updated)) => {
                    prepared = Some(updated);
                    continue;
                }
                Ok(None) => {
                    return QueryResponse::error(
                        "retry input resolution returned no preparation capsule",
                    )
                }
                Err(error) => {
                    return QueryResponse::error(
                        query::executor::QueryResponseError::from_query_error(error),
                    )
                }
            }
        }
        return response;
    }
}

/// Execute a GraphQL query within a transaction with signing config and DAC bypass context.
pub async fn execute_in_txn_with_context(
    state: &AppState,
    identity: &ExtractIdentity,
    mut request: QueryRequest,
    txn_handle: TransactionHandle,
) -> QueryResponse {
    request.context = request_context(state, identity).await;
    let context = request.context.clone();
    context
        .mirror(state.executor.execute_in_txn(request, &txn_handle))
        .await
}

/// Execute a query with pre-resolved context values, `acting_did` acting in
/// it.
///
/// Used by SSE streams where signing config and DAC bypass are resolved once
/// at subscription setup time, then reused for each per-event query. The
/// query runs on the calling task; `_edge` is unused.
pub async fn execute_with_resolved_context(
    _edge: &Edge,
    executor: Arc<dyn QueryExecutor>,
    mut request: QueryRequest,
    signing_config: Option<SigningConfig>,
    dac_bypass: bool,
    acting_did: Option<String>,
) -> QueryResponse {
    request.context = with_signing_and_bypass(
        request.context.with_identity(acting_did),
        signing_config,
        dac_bypass,
    );
    let context = request.context.clone();
    context.mirror(executor.execute(request)).await
}

/// The context of the request `identity` acts in, with the signing config,
/// batch session and DAC bypass it resolves to.
async fn request_context(
    state: &AppState,
    identity: &ExtractIdentity,
) -> defra_core::RequestContext {
    with_signing_and_bypass(
        identity.context(),
        resolve_signing_config(state, identity),
        resolve_dac_bypass(state, identity).await,
    )
}

/// `context` signing with `signing_config`, collecting into its batch session,
/// and with the DAC bypass `dac_bypass`.
fn with_signing_and_bypass(
    context: defra_core::RequestContext,
    signing_config: Option<SigningConfig>,
    dac_bypass: bool,
) -> defra_core::RequestContext {
    let batch_session = signing_config.as_ref().map(|s| s.public_key_hex.clone());
    context
        .with_dac_bypass(dac_bypass)
        .with_signing(signing_config)
        .with_batch_session(batch_session)
}

/// Resolve the signing config for a request identity.
///
/// - If identity has a DID, look up its signing config.
/// - If no DID (anonymous), fall back to the node identity DID.
pub(crate) fn resolve_signing_config(
    state: &AppState,
    identity: &ExtractIdentity,
) -> Option<SigningConfig> {
    defra_core::signing::resolve_signing_config_with_flag(
        identity.did().map(|d| d.as_str()),
        state.node_identity_did.as_deref(),
        state.signing_enabled,
    )
}

/// Resolve whether DAC bypass should be enabled for this request.
///
/// - If NAC is configured and the identity has `DacBypass` permission, bypass.
/// - Otherwise, no bypass.
pub(crate) async fn resolve_dac_bypass(state: &AppState, identity: &ExtractIdentity) -> bool {
    let Some(nac) = &state.nac else {
        return false;
    };

    let status = nac.get_status().await;
    acp::nac::should_bypass_dac(status, identity.did(), |d, p| async move {
        nac.check_permission(d, p).await.unwrap_or(false)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use query::error::{Result, TransactionError};

    use super::*;
    use crate::router::AppStateBuilder;

    #[derive(Default)]
    struct ConflictExecutor {
        preparations: AtomicUsize,
        auto_commit_attempts: AtomicUsize,
        explicit_txn_attempts: AtomicUsize,
        input_on_second: bool,
        input_resolutions: AtomicUsize,
        engine_managed_retry: bool,
    }

    #[async_trait]
    impl QueryExecutor for ConflictExecutor {
        async fn prepare_request(
            &self,
            _request: &QueryRequest,
        ) -> Result<Option<Arc<query::prepared::PreparedMutations>>> {
            self.preparations.fetch_add(1, Ordering::SeqCst);
            Ok(Some(Arc::new(query::prepared::PreparedMutations {
                engine_managed_retry: self.engine_managed_retry,
                ..Default::default()
            })))
        }

        fn abandon_txn(&self, _handle: &TransactionHandle) {}

        async fn execute(&self, _request: QueryRequest) -> QueryResponse {
            assert_eq!(self.preparations.load(Ordering::SeqCst), 1);
            let attempt = self.auto_commit_attempts.fetch_add(1, Ordering::SeqCst);
            if self.input_on_second {
                match attempt {
                    0 => QueryResponse::error(query::QueryResponseError::from_query_error(
                        query::error::QueryError::NeedsInput(
                            query::prepared::PreparedInputRequest::new(
                                query::prepared::PreparedInputKind::Embedding,
                                br#"{"model":"m","input":"source"}"#.as_slice(),
                            ),
                        ),
                    )),
                    1 => QueryResponse::transaction_conflict("transaction conflict"),
                    _ => QueryResponse::success(serde_json::json!({"_docID": "doc"})),
                }
            } else if attempt < 2 {
                QueryResponse::transaction_conflict("transaction conflict")
            } else {
                QueryResponse::success(serde_json::json!({"_docID": "doc"}))
            }
        }

        async fn resolve_retry_input(
            &self,
            _request: &QueryRequest,
            prepared: Option<Arc<query::prepared::PreparedMutations>>,
            input: &query::prepared::PreparedInputRequest,
        ) -> Result<Option<Arc<query::prepared::PreparedMutations>>> {
            self.input_resolutions.fetch_add(1, Ordering::SeqCst);
            let Some(prepared) = prepared else {
                return Ok(None);
            };
            if prepared.inputs.get(input).is_none() {
                prepared.inputs.insert(input.clone(), b"[1.0]".as_slice());
            }
            Ok(Some(prepared))
        }

        async fn execute_in_txn(
            &self,
            _request: QueryRequest,
            _handle: &TransactionHandle,
        ) -> QueryResponse {
            self.explicit_txn_attempts.fetch_add(1, Ordering::SeqCst);
            QueryResponse::transaction_conflict("transaction conflict")
        }

        async fn begin_txn(
            &self,
            _readonly: bool,
        ) -> std::result::Result<TransactionHandle, TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn commit_txn(
            &self,
            _handle: &TransactionHandle,
        ) -> std::result::Result<(), TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn discard_txn(
            &self,
            _handle: &TransactionHandle,
        ) -> std::result::Result<(), TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn schema(&self) -> Result<String> {
            Ok(String::new())
        }
    }

    #[nuclei::test]
    async fn auto_commit_returns_business_conflict_without_replaying_request() {
        let executor = Arc::new(ConflictExecutor::default());
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge()).build();

        let response = execute_with_context(
            &state,
            &ExtractIdentity::anonymous(),
            QueryRequest::new("mutation { create_User(input: {}) { _docID } }"),
        )
        .await;

        assert_eq!(executor.preparations.load(Ordering::SeqCst), 1);
        assert_eq!(executor.auto_commit_attempts.load(Ordering::SeqCst), 1);
        assert!(response.is_transaction_conflict());
    }

    #[nuclei::test]
    async fn engine_managed_batch_conflict_is_not_replayed_by_http() {
        let executor = Arc::new(ConflictExecutor {
            engine_managed_retry: true,
            ..Default::default()
        });
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge()).build();

        let response = execute_with_context(
            &state,
            &ExtractIdentity::anonymous(),
            QueryRequest::new("mutation { first: create_User(input: {}) { _docID } second: create_User(input: {}) { _docID } }"),
        )
        .await;

        assert!(response.is_transaction_conflict());
        assert_eq!(executor.preparations.load(Ordering::SeqCst), 1);
        assert_eq!(executor.auto_commit_attempts.load(Ordering::SeqCst), 1);
    }

    #[nuclei::test]
    async fn auto_commit_resolves_input_then_returns_conflict_without_replaying_it() {
        let executor = Arc::new(ConflictExecutor {
            input_on_second: true,
            ..Default::default()
        });
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge()).build();

        let response = execute_with_context(
            &state,
            &ExtractIdentity::anonymous(),
            QueryRequest::new("mutation { update_User(input: {}) { _docID } }"),
        )
        .await;

        assert_eq!(executor.preparations.load(Ordering::SeqCst), 1);
        assert_eq!(executor.auto_commit_attempts.load(Ordering::SeqCst), 2);
        assert_eq!(executor.input_resolutions.load(Ordering::SeqCst), 1);
        assert!(response.is_transaction_conflict());
    }

    #[nuclei::test]
    async fn explicit_transaction_execution_is_not_retried() {
        let executor = Arc::new(ConflictExecutor::default());
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge()).build();

        let response = execute_in_txn_with_context(
            &state,
            &ExtractIdentity::anonymous(),
            QueryRequest::new("mutation { update_User(input: {}) { _docID } }"),
            TransactionHandle::new("txn".to_string()),
        )
        .await;

        assert_eq!(executor.explicit_txn_attempts.load(Ordering::SeqCst), 1);
        assert!(response.is_transaction_conflict());
    }

    const OWNER_DID: &str = "did:key:z6MkTestNodeOwner";

    /// Records the `dac_bypass` thread-local as observed by the executor, which
    /// is the only place the fast path's effect is visible.
    #[derive(Default)]
    struct DacBypassRecorder {
        observed: std::sync::atomic::AtomicBool,
        bearer: kovan::Atom<Option<String>>,
    }

    #[async_trait]
    impl QueryExecutor for DacBypassRecorder {
        fn abandon_txn(&self, _handle: &TransactionHandle) {}

        async fn execute(&self, _request: QueryRequest) -> QueryResponse {
            self.observed
                .store(defra_core::dac_bypass::get_dac_bypass(), Ordering::SeqCst);
            self.bearer
                .store(defra_core::signing::get_request_bearer_token(OWNER_DID));
            QueryResponse::success(serde_json::json!({}))
        }

        async fn execute_in_txn(
            &self,
            _request: QueryRequest,
            _handle: &TransactionHandle,
        ) -> QueryResponse {
            self.observed
                .store(defra_core::dac_bypass::get_dac_bypass(), Ordering::SeqCst);
            self.bearer
                .store(defra_core::signing::get_request_bearer_token(OWNER_DID));
            QueryResponse::success(serde_json::json!({}))
        }

        async fn begin_txn(
            &self,
            _readonly: bool,
        ) -> std::result::Result<TransactionHandle, TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn commit_txn(
            &self,
            _handle: &TransactionHandle,
        ) -> std::result::Result<(), TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn discard_txn(
            &self,
            _handle: &TransactionHandle,
        ) -> std::result::Result<(), TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn schema(&self) -> Result<String> {
            Ok(String::new())
        }
    }

    fn owner_identity() -> ExtractIdentity {
        ExtractIdentity::from_did(Some(identity::Did::new_unchecked(OWNER_DID.to_string())))
    }

    #[nuclei::test]
    async fn node_owner_is_not_promoted_to_nac_dac_bypass() {
        let executor = Arc::new(DacBypassRecorder::default());
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge())
            .with_node_identity_did(OWNER_DID.to_string())
            .with_signing_enabled(false)
            .build();

        execute_with_context(
            &state,
            &owner_identity(),
            QueryRequest::new("{ __typename }"),
        )
        .await;

        assert!(!executor.observed.load(Ordering::SeqCst));
    }

    #[nuclei::test]
    async fn a_stranger_gets_no_dac_bypass() {
        let executor = Arc::new(DacBypassRecorder::default());
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge())
            .with_node_identity_did(OWNER_DID.to_string())
            .with_signing_enabled(false)
            .build();

        execute_with_context(
            &state,
            &ExtractIdentity::from_did(Some(identity::Did::new_unchecked(
                "did:key:z6MkSomeoneElse".to_string(),
            ))),
            QueryRequest::new("{ __typename }"),
        )
        .await;

        assert!(!executor.observed.load(Ordering::SeqCst));
    }
    #[nuclei::test]
    async fn bearer_scope_crosses_each_blocking_execution_boundary() {
        let executor = Arc::new(DacBypassRecorder::default());
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge())
            .with_signing_enabled(false)
            .with_nac(Arc::new(crate::MockNodeAcpOperations::new()))
            .build();
        for path in 0..3 {
            executor.bearer.store(None);
            defra_core::signing::scope_request_bearer_token(
                Some(OWNER_DID.to_string()),
                Some("request-token".to_string()),
                async {
                    let request = QueryRequest::new("{ __typename }");
                    match path {
                        0 => execute_with_context(&state, &owner_identity(), request).await,
                        1 => {
                            execute_in_txn_with_context(
                                &state,
                                &owner_identity(),
                                request,
                                TransactionHandle::new("txn".into()),
                            )
                            .await
                        }
                        _ => {
                            execute_with_resolved_context(
                                &state.edge,
                                executor.clone(),
                                request,
                                None,
                                false,
                                Some(OWNER_DID.to_string()),
                            )
                            .await
                        }
                    }
                },
            )
            .await;
            assert_eq!(
                executor.bearer.load_clone().as_deref(),
                Some("request-token")
            );
        }
    }
}

#[cfg(test)]
mod context_tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use kovan_queue::seg_queue::SegQueue;
    use query::error::{Result, TransactionError};

    use super::*;
    use crate::router::AppStateBuilder;

    const OWNER_DID: &str = "did:key:z6MkTestNodeOwner";
    const ADMIN_DID: &str = "did:key:z6MkDacBypassAdmin";

    /// Records the context of every request it serves.
    #[derive(Default)]
    struct ContextRecorder {
        contexts: SegQueue<defra_core::RequestContext>,
    }

    impl ContextRecorder {
        fn last(&self) -> Option<defra_core::RequestContext> {
            std::iter::from_fn(|| self.contexts.pop()).last()
        }
    }

    #[async_trait]
    impl QueryExecutor for ContextRecorder {
        fn abandon_txn(&self, _handle: &TransactionHandle) {}

        async fn execute(&self, request: QueryRequest) -> QueryResponse {
            self.contexts.push(request.context);
            QueryResponse::success(serde_json::json!({}))
        }

        async fn execute_in_txn(
            &self,
            request: QueryRequest,
            _handle: &TransactionHandle,
        ) -> QueryResponse {
            self.contexts.push(request.context);
            QueryResponse::success(serde_json::json!({}))
        }

        async fn begin_txn(
            &self,
            _readonly: bool,
        ) -> std::result::Result<TransactionHandle, TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn commit_txn(
            &self,
            _handle: &TransactionHandle,
        ) -> std::result::Result<(), TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn discard_txn(
            &self,
            _handle: &TransactionHandle,
        ) -> std::result::Result<(), TransactionError> {
            Err(TransactionError::not_supported("test executor"))
        }

        async fn schema(&self) -> Result<String> {
            Ok(String::new())
        }
    }

    #[nuclei::test]
    async fn a_nac_admin_with_bypass_dac_carries_the_bypass_and_identity_in_its_request() {
        let admin = identity::Did::new_unchecked(ADMIN_DID.to_string());
        let executor = Arc::new(ContextRecorder::default());
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge())
            .with_signing_enabled(false)
            .with_nac(Arc::new(
                crate::MockNodeAcpOperations::enabled_with_owner(identity::Did::new_unchecked(
                    OWNER_DID.to_string(),
                ))
                .with_grant(admin.clone(), acp::nac::NodePermission::DacBypass),
            ))
            .build();
        let caller = ExtractIdentity::from_did(Some(admin));

        execute_with_context(&state, &caller, QueryRequest::new("{ __typename }")).await;
        let context = executor.last().expect("the query ran");
        assert!(context.dac_bypass());
        assert_eq!(context.identity(), Some(ADMIN_DID));

        execute_in_txn_with_context(
            &state,
            &caller,
            QueryRequest::new("{ __typename }"),
            TransactionHandle::new("txn".into()),
        )
        .await;
        let context = executor.last().expect("the query ran");
        assert!(context.dac_bypass());
        assert_eq!(context.identity(), Some(ADMIN_DID));

        execute_with_resolved_context(
            &state.edge,
            executor.clone(),
            QueryRequest::new("{ __typename }"),
            None,
            true,
            Some(ADMIN_DID.to_string()),
        )
        .await;
        let context = executor.last().expect("the query ran");
        assert!(context.dac_bypass());
        assert_eq!(context.identity(), Some(ADMIN_DID));
    }

    #[nuclei::test]
    async fn a_stranger_carries_no_bypass_in_its_request() {
        let executor = Arc::new(ContextRecorder::default());
        let state = AppStateBuilder::new(executor.clone(), edge::testing::edge())
            .with_signing_enabled(false)
            .with_nac(Arc::new(crate::MockNodeAcpOperations::enabled_with_owner(
                identity::Did::new_unchecked(OWNER_DID.to_string()),
            )))
            .build();
        let stranger = ExtractIdentity::from_did(Some(identity::Did::new_unchecked(
            "did:key:z6MkSomeoneElse".to_string(),
        )));

        execute_with_context(&state, &stranger, QueryRequest::new("{ __typename }")).await;
        let context = executor.last().expect("the query ran");
        assert!(!context.dac_bypass());
        assert_eq!(context.identity(), Some("did:key:z6MkSomeoneElse"));
    }
}
