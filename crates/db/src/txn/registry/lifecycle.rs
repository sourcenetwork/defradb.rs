//! Transaction begin, requests, commit, discard and abandon, each a message
//! to the worker that owns the transaction.

use super::*;
use async_trait::async_trait;
use query::txn::{TransactionHandle, TransactionRegistry, TxnJob};
use storage::corekv::MaybeSend;
use web_time::Instant;

fn report_slow_phase(operation: &'static str, phase: &'static str, started: Instant) {
    let elapsed = started.elapsed();
    if elapsed >= Duration::from_secs(1) {
        tracing::warn!(
            target: "defradb.transaction_phase",
            operation,
            phase,
            elapsed_ms = elapsed.as_millis() as u64,
            "slow native transaction owner phase"
        );
    }
}

/// Runs `job` until it finishes or `timeout` passes, when the job is dropped
/// and the request fails.
async fn within<F: std::future::Future>(
    timeout: Option<Duration>,
    job: F,
) -> std::result::Result<F::Output, TransactionError> {
    query::runner::within_deadline(timeout, job)
        .await
        .map_err(TransactionError::execution)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> TransactionRegistry for DbTransactionRegistry<S> {
    fn abandon(&self, handle: &TransactionHandle) {
        if let (Ok(owners), Some(id)) = (self.owners(), TxnId::parse(handle)) {
            owners.abandon(id);
        }
    }

    async fn begin(
        &self,
        readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        self.begin_as(&RequestContext::ambient(), readonly).await
    }

    async fn begin_as(
        &self,
        context: &RequestContext,
        readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        let queued = Instant::now();
        let db = self.db.clone();
        let broadcaster = self.broadcaster.clone();
        let opened = self
            .owners()
            .map_err(|error| TransactionError::execution(error.to_string()))?
            .begin(caller(context), move |id| async move {
                report_slow_phase("begin", "queue", queued);
                let started = Instant::now();
                let result = db.new_txn(readonly).await;
                report_slow_phase("begin", "execute", started);
                let mut txn = result?;
                txn.make_explicit();
                OwnedTxn::new(id, txn, db, broadcaster)
            })
            .await
            .map_err(|error| TransactionError::execution(error.to_string()))?;
        let id = opened.map_err(crate::error::transaction_error)?;
        Ok(TransactionHandle::new(id.to_string()))
    }

    async fn run<T>(
        &self,
        handle: &TransactionHandle,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        self.run_as(&RequestContext::ambient(), handle, timeout, job)
            .await
    }

    async fn run_as<T>(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        let id = TxnId::parse(handle).ok_or_else(|| {
            TransactionError::not_found(format!("transaction '{handle}' not found"))
        })?;
        let scope = context.clone();
        let queued = Instant::now();
        self.owners()
            .map_err(|error| unavailable(error, handle))?
            .run(id, &caller(context), move |owned| {
                Box::pin(async move {
                    report_slow_phase("run", "queue", queued);
                    let started = Instant::now();
                    let ran = scope.mirror(within(timeout, job(owned))).await;
                    report_slow_phase("run", "execute", started);
                    owned.after_request();
                    ran
                })
            })
            .await
            .map_err(|error| unavailable(error, handle))?
    }

    async fn read<T>(
        &self,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        self.read_as(&RequestContext::ambient(), timeout, job).await
    }

    async fn read_as<T>(
        &self,
        context: &RequestContext,
        timeout: Option<Duration>,
        job: TxnJob<T>,
    ) -> std::result::Result<T, TransactionError>
    where
        T: MaybeSend + 'static,
    {
        let queued = Instant::now();
        let db = self.db.clone();
        let scope = context.clone();
        self.owners()
            .map_err(|error| TransactionError::execution(error.to_string()))?
            .run_anywhere("txn.read", move || async move {
                report_slow_phase("read", "queue", queued);
                let started = Instant::now();
                let result = scope
                    .mirror(async move {
                        let txn = db
                            .new_txn(true)
                            .await
                            .map_err(crate::error::transaction_error)?;
                        // A snapshot of its own, never registered: no ID names it.
                        let mut owned = OwnedTxn::new(TxnId::unnamed(), txn, db, None)
                            .map_err(|error| TransactionError::execution(error.to_string()))?;
                        within(timeout, job(&mut owned)).await
                    })
                    .await;
                report_slow_phase("read", "execute", started);
                result
            })
            .await
            .map_err(|error| TransactionError::execution(error.to_string()))?
    }

    async fn commit(
        &self,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.commit_as(&RequestContext::ambient(), handle).await
    }

    async fn commit_as(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        let id = TxnId::parse(handle).ok_or_else(|| {
            TransactionError::not_found(format!("transaction '{handle}' not found"))
        })?;
        let label = handle.to_string();
        let queued = Instant::now();
        self.owners()
            .map_err(|error| unavailable(error, handle))?
            .finish(id, &caller(context), move |owned| {
                Box::pin(async move {
                    report_slow_phase("commit", "queue", queued);
                    let started = Instant::now();
                    let result = owned.commit().await;
                    report_slow_phase("commit", "execute", started);
                    result.map_err(|error| commit_error(&label, error))
                })
            })
            .await
            .map_err(|error| unavailable(error, handle))?
    }

    async fn add_collection_as(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> std::result::Result<Vec<schema::CollectionVersion>, TransactionError> {
        let id = context_txn(context)?;
        self.add_collection_in_txn_as(context, &id, sdl)
            .await
            .map_err(operation_error)
    }

    async fn get_collections_as(
        &self,
        context: &RequestContext,
        selector: &schema::CollectionSelector,
    ) -> std::result::Result<Vec<schema::CollectionVersion>, TransactionError> {
        let Some(id) = context.txn() else {
            return self
                .db
                .get_collections_as(context, selector)
                .await
                .map_err(operation_error);
        };
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::CollectionGet)
            .await
            .map_err(operation_error)?;
        let versions = self
            .get_collections_in_txn_as(context, &id.to_string())
            .await
            .map_err(operation_error)?;
        crate::collection::ops::select(versions, selector).map_err(operation_error)
    }

    async fn patch_collection_as(
        &self,
        context: &RequestContext,
        patch: &str,
        migration: Option<lens::LensConfig>,
    ) -> std::result::Result<(), TransactionError> {
        if context.txn().is_some() {
            return Err(TransactionError::not_supported(
                "collection patches inside an explicit transaction are not supported yet",
            ));
        }
        self.db
            .patch_collections_as(context, patch, migration)
            .await
            .map_err(operation_error)
    }

    async fn truncate_collection_as(
        &self,
        context: &RequestContext,
        name: &str,
        filter: Option<&query::documents::Filter>,
    ) -> std::result::Result<(), TransactionError> {
        self.db
            .truncate_collection_as(context, name, filter)
            .await
            .map_err(|error| TransactionError::operation(error.to_string()))
    }

    async fn add_dac_policy_as(
        &self,
        context: &RequestContext,
        policy: &str,
    ) -> std::result::Result<String, TransactionError> {
        self.db
            .add_dac_policy_as(context, policy)
            .await
            .map_err(|error| TransactionError::operation(error.to_string()))
    }

    async fn list_dac_policies_as(
        &self,
        context: &RequestContext,
    ) -> std::result::Result<Vec<acp::Policy>, TransactionError> {
        self.db
            .list_dac_policies_as(context)
            .await
            .map_err(|error| TransactionError::operation(error.to_string()))
    }

    async fn get_dac_policy_as(
        &self,
        context: &RequestContext,
        policy_id: &str,
    ) -> std::result::Result<Option<acp::Policy>, TransactionError> {
        self.db
            .get_dac_policy_as(context, policy_id)
            .await
            .map_err(|error| TransactionError::operation(error.to_string()))
    }

    async fn acp_light_client_status_as(
        &self,
        context: &RequestContext,
    ) -> std::result::Result<acp::LightClientStatus, TransactionError> {
        self.db
            .acp_light_client_status_as(context)
            .await
            .map_err(|error| TransactionError::operation(error.to_string()))
    }

    async fn set_active_collection_version_as(
        &self,
        context: &RequestContext,
        version_id: &str,
    ) -> std::result::Result<(), TransactionError> {
        let Some(id) = context.txn() else {
            return self
                .db
                .set_active_collection_version_as(context, version_id)
                .await
                .map_err(operation_error);
        };
        self.set_collection_active_in_txn_as(context, &id.to_string(), version_id, true)
            .await
            .map(|_| ())
            .map_err(operation_error)
    }

    async fn set_migration_as(
        &self,
        context: &RequestContext,
        config: &str,
    ) -> std::result::Result<String, TransactionError> {
        let config: lens::LensConfig = serde_json::from_str(config).map_err(|e| {
            TransactionError::operation(format!("failed to parse lens config: {e}"))
        })?;
        let transform = match context.txn() {
            Some(id) => {
                self.set_migration_in_txn_as(context, &id.to_string(), config)
                    .await
            }
            None => self.db.set_migration_as(context, config).await,
        };
        transform
            .map(|transform| transform.to_string())
            .map_err(operation_error)
    }

    async fn add_lens_as(
        &self,
        context: &RequestContext,
        config: lens::LensConfig,
    ) -> std::result::Result<String, TransactionError> {
        let transform = match context.txn() {
            Some(id) => {
                self.add_lens_in_txn_as(context, &id.to_string(), config)
                    .await
            }
            None => self.db.add_lens_as(context, config).await,
        };
        transform
            .map(|transform| transform.to_string())
            .map_err(operation_error)
    }

    async fn list_lenses_as(
        &self,
        context: &RequestContext,
    ) -> std::result::Result<String, TransactionError> {
        let lenses = match context.txn() {
            Some(id) => self.list_lenses_in_txn_as(context, &id.to_string()).await,
            None => self.db.list_lenses_as(context).await,
        }
        .map_err(operation_error)?;
        // Go's `map[string]model.Lens`: each lens is its modules, in order.
        let lenses: serde_json::Map<String, serde_json::Value> = lenses
            .into_iter()
            .map(|(id, modules)| (id, serde_json::json!({ "Lenses": modules })))
            .collect();
        Ok(serde_json::Value::Object(lenses).to_string())
    }

    async fn delete_collections_as(
        &self,
        context: &RequestContext,
        targets: Vec<String>,
        active_only: bool,
    ) -> std::result::Result<(), TransactionError> {
        let id = context_txn(context)?;
        self.delete_collections_in_txn_as(context, &id, targets, active_only)
            .await
            .map_err(operation_error)
    }

    async fn set_collection_active_as(
        &self,
        context: &RequestContext,
        version_id: &str,
        is_active: bool,
    ) -> std::result::Result<schema::CollectionVersion, TransactionError> {
        let id = context_txn(context)?;
        self.set_collection_active_in_txn_as(context, &id, version_id, is_active)
            .await
            .map_err(operation_error)
    }

    async fn verify_block_signature_as(
        &self,
        context: &RequestContext,
        document_acp: std::sync::Arc<dyn acp::DocumentACP>,
        cid: &str,
        public_key: &str,
        key_type: &str,
        caller: &acp::Identity,
    ) -> std::result::Result<(), TransactionError> {
        let id = context_txn(context)?;
        let key_type = match key_type {
            "ed25519" => crypto::KeyType::Ed25519,
            "secp256k1" => crypto::KeyType::Secp256k1,
            "secp256r1" => crypto::KeyType::Secp256r1,
            other => {
                return Err(TransactionError::operation(format!(
                    "unsupported key type: {other}"
                )))
            }
        };
        self.verify_block_signature_in_txn_as(
            context,
            &id,
            document_acp,
            cid,
            public_key,
            key_type,
            caller,
        )
        .await
        .map_err(operation_error)
    }

    async fn discard(
        &self,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.discard_as(&RequestContext::ambient(), handle).await
    }

    async fn discard_as(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        let id = TxnId::parse(handle).ok_or_else(|| {
            TransactionError::not_found(format!("transaction '{handle}' not found"))
        })?;
        let label = handle.to_string();
        self.owners()
            .map_err(|error| unavailable(error, handle))?
            .finish(id, &caller(context), move |owned| {
                Box::pin(async move {
                    owned.discard().map_err(|error| {
                        TransactionError::execution(format!(
                            "discard error for transaction '{label}': {error}"
                        ))
                    })
                })
            })
            .await
            .map_err(|error| unavailable(error, handle))?
    }
}

/// A commit that lost a write conflict is a typed, retryable conflict.
fn commit_error(handle: &str, error: crate::error::Error) -> TransactionError {
    let message = format!("commit error for transaction '{handle}': {error}");
    if error.is_txn_conflict() {
        TransactionError::conflict(message)
    } else {
        match crate::error::transaction_error(error) {
            TransactionError::Storage(storage::Error::Io(cause)) => {
                TransactionError::Storage(storage::Error::Io(
                    cause.with_context(format!("commit error for transaction '{handle}'")),
                ))
            }
            _ => TransactionError::execution(message),
        }
    }
}

/// The transaction `context` runs in, as the text the registry's tables key
/// on; a context without one names no transaction.
fn context_txn(context: &RequestContext) -> std::result::Result<String, TransactionError> {
    context
        .txn()
        .map(|id| id.to_string())
        .ok_or_else(|| TransactionError::not_found("the request carries no transaction"))
}

/// A refused operation inside a transaction, in the words the core gave it;
/// a missing or foreign transaction keeps its own variant.
fn operation_error(error: crate::error::Error) -> TransactionError {
    match error {
        crate::error::Error::TransactionNotFound(message) => TransactionError::not_found(message),
        other => TransactionError::operation(other.to_string()),
    }
}
