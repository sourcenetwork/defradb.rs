use std::ffi::{c_char, c_int};

use acp::nac::NodePermission;

use crate::call::{self, DefraResultCallback, Reply};
use crate::ffi_entry;
use crate::helpers::{get_node_runner, get_rt, require_c_str};
use crate::nac_check::admit_on_node;
use crate::state::{GraphQLSubscriptionState, GRAPHQL_SUBSCRIPTIONS, NODES};
use crate::types::{c_str_to_string, FfiResult};
use crate::{try_ffi, ERR_INVALID_NODE_HANDLE};

use super::{
    dac_bypass, nac_permission_for_query, subscription_accepts_doc_id, subscription_doc_ids,
    subscription_to_scoped_query,
};

/// Execute a GraphQL query or mutation.
///
/// Returns a JSON object with the query result in GraphQL format:
/// ```json
/// {
///     "data": { ... },
///     "errors": [ ... ]
/// }
/// ```
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `txn_id` - Optional transaction ID from `begin_txn` (null to commit on its own)
/// * `identity_did` - Optional DID string for ACP permission checks (null for anonymous)
/// * `request_query` - GraphQL query string (required)
/// * `operation_name` - Optional operation name for multi-operation documents (null if not used)
/// * `variables` - Optional JSON string of variables (null if not used)
/// * `batch_session_id` - Optional batch session ID for CID collection (null if not in batch mode).
///   When provided, CIDs created during this request are collected under this session.
///
/// # Safety
///
/// All string pointers must be either null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn exec_request(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    request_query: *const c_char,
    operation_name: *const c_char,
    variables: *const c_char,
    batch_session_id: *const c_char,
) -> FfiResult {
    unsafe {
        exec_request_with_signing(
            node_ptr,
            txn_id,
            identity_did,
            request_query,
            operation_name,
            variables,
            batch_session_id,
            -1,
        )
    }
}

/// Execute a GraphQL query or mutation with a request-scoped signing override.
///
/// `signing_override` accepts `-1` for the node default, `0` to disable signing,
/// and `1` to enable signing.
///
/// # Safety
///
/// All string pointers must be either null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn exec_request_with_signing(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    request_query: *const c_char,
    operation_name: *const c_char,
    variables: *const c_char,
    batch_session_id: *const c_char,
    signing_override: c_int,
) -> FfiResult {
    ffi_entry! {
        let args = try_ffi!(ExecArgs::read(
            node_ptr,
            txn_id,
            identity_did,
            request_query,
            operation_name,
            variables,
            batch_session_id,
            signing_override,
        ));
        call::wait(async move {
            Ok(match prepare(args).await {
                Ok((runner, request, txn)) => {
                    FfiResult::from(respond(run(&runner, request, txn).await))
                }
                Err(decided) => decided,
            })
        })
        .unwrap_or_else(FfiResult::error)
    }
}

/// [`exec_request_with_signing`] for a caller that is called back instead of
/// waiting: returns once the request is admitted to the node's worker pool,
/// and `callback` receives `context` and the result exactly once, as
/// [`exec_request_with_signing`] would return it, on the DefraDB thread that
/// ran the request. A request refused before it runs (an invalid argument, a
/// denied permission) and a subscription, which answers at once with its
/// handle, call it on the caller's thread before this returns.
///
/// # Safety
///
/// All string pointers must be either null or valid null-terminated UTF-8
/// strings, read only before this returns.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn exec_request_async(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    request_query: *const c_char,
    operation_name: *const c_char,
    variables: *const c_char,
    batch_session_id: *const c_char,
    signing_override: c_int,
    callback: DefraResultCallback,
    context: usize,
) {
    let reply = Reply::new(callback, context);
    // A panic drops `reply`, which reports the call ended without a result.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        match ExecArgs::read(
            node_ptr,
            txn_id,
            identity_did,
            request_query,
            operation_name,
            variables,
            batch_session_id,
            signing_override,
        ) {
            Ok(args) => call::detach(async move {
                match prepare(args).await {
                    Ok((runner, request, None)) => {
                        let done = move |response| reply.send(FfiResult::from(respond(response)));
                        runner.execute_then(request, Box::new(done)).await
                    }
                    Ok((runner, request, txn)) => {
                        let response = run(&runner, request, txn).await;
                        reply.send(FfiResult::from(respond(response)))
                    }
                    Err(decided) => reply.send(decided),
                }
            }),
            Err(decided) => reply.send(decided),
        }
    }));
}

/// The request path FFI's document writes run on: an exec call of
/// `mutation` as `identity`, with the node's signing and `batch_session`.
pub(crate) async fn run_mutation(
    node_ptr: usize,
    identity: Option<String>,
    batch_session: Option<String>,
    mutation: String,
) -> query::QueryResponse {
    let args = ExecArgs {
        node_ptr,
        txn: None,
        identity,
        query: mutation,
        operation_name: None,
        variables: None,
        batch_session,
        signing_override: -1,
    };
    match prepare(args).await {
        Ok((runner, request, txn)) => run(&runner, request, txn).await,
        Err(decided) => query::QueryResponse::error(decided.into_error().unwrap_or_default()),
    }
}

/// The request path a request the core built runs on (a typed mutation, not
/// GraphQL text): admitted under `permission` and run as `identity`, in the
/// transaction `txn` names or on its own, with the node's signing, as an
/// exec call is.
pub(crate) async fn run_typed(
    node_ptr: usize,
    identity: Option<String>,
    txn: Option<String>,
    permission: NodePermission,
    request: query::QueryRequest,
) -> query::QueryResponse {
    let caller = match Caller::admit(node_ptr, identity, txn, None, -1, permission).await {
        Ok(caller) => caller,
        Err(decided) => {
            return query::QueryResponse::error(decided.into_error().unwrap_or_default())
        }
    };
    let runner = match get_node_runner(node_ptr) {
        Ok(runner) => runner,
        Err(error) => return query::QueryResponse::error(error.into_error().unwrap_or_default()),
    };
    let mut request = request.with_context(caller.context);
    if caller.did.is_some() {
        request = request.with_identity(caller.did);
    }
    let handle = caller
        .txn_id
        .map(|id| query::txn::TransactionHandle::new(id.to_string()));
    run(&runner, request, handle).await
}

/// The context a call that names `permission` acts in, once node access
/// control admits `identity` for it: as an exec call's, with its identity,
/// its transaction, its DAC bypass and its signer.
pub(crate) async fn caller_context(
    node_ptr: usize,
    identity: Option<String>,
    txn: Option<String>,
    permission: NodePermission,
) -> Result<defra_core::RequestContext, String> {
    Caller::admit(node_ptr, identity, txn, None, -1, permission)
        .await
        .map(|caller| caller.context)
        .map_err(|decided| decided.into_error().unwrap_or_default())
}

/// Who an exec call acts as, admitted under the node's access control.
struct Caller {
    did: Option<identity::Did>,
    txn_id: Option<defra_core::TxnId>,
    context: defra_core::RequestContext,
}

impl Caller {
    /// Check `identity` holds `permission`, then build the context it acts
    /// in: its identity, its transaction, its DAC bypass and the signer
    /// that signs its writes.
    async fn admit(
        node_ptr: usize,
        identity: Option<String>,
        txn: Option<String>,
        batch_session: Option<String>,
        signing_override: c_int,
        permission: NodePermission,
    ) -> Result<Self, FfiResult> {
        admit_on_node(node_ptr, identity.as_deref(), permission).await?;

        let did = match identity {
            Some(ref s) if !s.is_empty() => match identity::Did::new(s) {
                Ok(d) => Some(d),
                Err(e) => return Err(FfiResult::error(format!("invalid identity DID: {}", e))),
            },
            _ => None,
        };

        // The signer for block signing during mutations. Matches Go's
        // behavior: if signing is enabled and no explicit identity, fall back
        // to node identity for block signing.
        let (node_did, node_signing_enabled) = NODES
            .get(node_ptr, |state| {
                (state.identity_did(), state.signing_enabled)
            })
            .unwrap_or((None, false));
        let signing_enabled = match signing_override {
            -1 => node_signing_enabled,
            0 => false,
            1 => true,
            _ => return Err(FfiResult::error("signing_override must be -1, 0, or 1")),
        };
        let signing = defra_core::signing::resolve_signing_config_with_flag(
            identity.as_deref(),
            node_did.as_deref(),
            signing_enabled,
        );
        // Use caller-provided session ID if available; otherwise fall back to public key.
        let session_key =
            batch_session.or_else(|| signing.as_ref().map(|s| s.public_key_hex.clone()));

        // The caller acts in the request's context, so DB-layer NAC checks
        // resolve it; a NAC admin/owner can read all documents.
        let txn_id = match txn.as_deref() {
            Some(id) => Some(
                id.parse::<defra_core::TxnId>()
                    .map_err(|e| FfiResult::error(format!("invalid transaction ID: {}", e)))?,
            ),
            None => None,
        };
        let context = defra_core::RequestContext::default()
            .with_txn(txn_id)
            .with_identity(identity.as_ref().filter(|s| !s.is_empty()).cloned())
            .with_dac_bypass(dac_bypass(node_ptr, identity.as_deref()).await)
            .with_signing(signing)
            .with_batch_session(session_key);
        Ok(Self {
            did,
            txn_id,
            context,
        })
    }
}

/// Runs `request` in the transaction it names, or on its own.
async fn run(
    runner: &std::sync::Arc<dyn query::QueryExecutor>,
    request: query::QueryRequest,
    txn: Option<query::txn::TransactionHandle>,
) -> query::QueryResponse {
    match txn {
        Some(handle) => runner.execute_in_txn(request, &handle).await,
        None => runner.execute(request).await,
    }
}

/// The response's GraphQL JSON.
fn respond(response: query::QueryResponse) -> Result<String, String> {
    serde_json::to_string(&response).map_err(|e| format!("failed to serialize response: {}", e))
}

/// An exec call's arguments, read from the caller's strings before the call
/// returns.
struct ExecArgs {
    node_ptr: usize,
    txn: Option<String>,
    identity: Option<String>,
    query: String,
    operation_name: Option<String>,
    variables: Option<String>,
    batch_session: Option<String>,
    signing_override: c_int,
}

impl ExecArgs {
    /// # Safety
    ///
    /// As for [`exec_request_with_signing`].
    #[allow(clippy::too_many_arguments)]
    unsafe fn read(
        node_ptr: usize,
        txn_id: *const c_char,
        identity_did: *const c_char,
        request_query: *const c_char,
        operation_name: *const c_char,
        variables: *const c_char,
        batch_session_id: *const c_char,
        signing_override: c_int,
    ) -> Result<Self, FfiResult> {
        Ok(Self {
            node_ptr,
            txn: c_str_to_string(txn_id).filter(|id| !id.is_empty()),
            identity: c_str_to_string(identity_did),
            query: require_c_str(request_query, "request_query")?,
            operation_name: c_str_to_string(operation_name).filter(|name| !name.is_empty()),
            variables: c_str_to_string(variables),
            batch_session: c_str_to_string(batch_session_id),
            signing_override,
        })
    }
}

/// Checks an exec call and returns the node's executor with the request to
/// run on it. `Err` is the call's result when it is decided before any
/// request runs: an error, or a subscription's handle.
async fn prepare(
    args: ExecArgs,
) -> Result<
    (
        std::sync::Arc<dyn query::QueryExecutor>,
        query::QueryRequest,
        Option<query::txn::TransactionHandle>,
    ),
    FfiResult,
> {
    let ExecArgs {
        node_ptr,
        txn,
        identity: identity_str,
        query: query_str,
        operation_name: op_name,
        variables: vars_str,
        batch_session,
        signing_override,
    } = args;

    let permission = nac_permission_for_query(&query_str);
    let Caller {
        did,
        txn_id,
        context,
    } = Caller::admit(
        node_ptr,
        identity_str.clone(),
        txn,
        batch_session,
        signing_override,
        permission,
    )
    .await?;

    // Check if this is a subscription query - handle it separately from regular queries/mutations.
    // Subscriptions return status=2 with a handle that the Go side polls via poll_graphql_subscription.
    let trimmed_query = query_str.trim_start();
    if trimmed_query.starts_with("subscription") {
        // Validate before subscribing. The per-event path below can only
        // log a document it will always reject, leaving the caller polling
        // a handle that never yields.
        if let Err(error) =
            query::subscription::validate_subscription(&query_str, op_name.as_deref())
        {
            return Err(FfiResult::error(error.to_string()));
        }

        // Create an event bus subscription to receive update events
        let mut subscription = match NODES.get(node_ptr, |state| {
            state.event_bus.subscribe(&[events::EventName::Update])
        }) {
            Some(sub) => sub,
            None => return Err(FfiResult::error(ERR_INVALID_NODE_HANDLE)),
        };
        let sub_id = subscription.id();

        let query_runner = match NODES.get(node_ptr, |state| state.query_runner.clone()) {
            Some(r) => r,
            None => return Err(FfiResult::error(ERR_INVALID_NODE_HANDLE)),
        };

        let subscription_variables = match vars_str.as_ref() {
            Some(vars) => match serde_json::from_str::<serde_json::Value>(vars) {
                Ok(value) => Some(value),
                Err(e) => {
                    return Err(FfiResult::error(format!(
                        "failed to parse variables: {}",
                        e
                    )))
                }
            },
            None => None,
        };
        let subscription_doc_ids_filter = subscription_doc_ids(
            &query_str,
            subscription_variables.as_ref(),
            op_name.as_deref(),
        );
        let sub_query = query_str.clone();
        let sub_identity = did.clone();
        let sub_operation_name = op_name.clone();
        let sub_variables = subscription_variables.clone();

        // Create result channel for buffering processed subscription results
        let (result_tx, result_rx) = kovan_channel::bounded::<String>(256);

        // A background service processes events and executes queries at event time.
        // This ensures the DB state at query execution matches the event's state.
        let service = get_rt()?.spawn_service(async move {
            while let Some(message) = subscription.recv().await {
                if let Some(update) = message.as_update() {
                    let event_doc_id = update.doc_id.clone();

                    if !subscription_accepts_doc_id(
                        subscription_doc_ids_filter.as_deref(),
                        &event_doc_id,
                    ) {
                        continue;
                    }

                    let cid_str = update.cid.to_string();
                    let query_text = match subscription_to_scoped_query(
                        &sub_query,
                        &event_doc_id,
                        &cid_str,
                        sub_operation_name.as_deref(),
                    ) {
                        Ok(query) => query,
                        Err(error) => {
                            tracing::warn!(error = %error, "failed to scope subscription query");
                            continue;
                        }
                    };

                    let mut request =
                        query::QueryRequest::new(query_text).with_context(context.clone());
                    if sub_identity.is_some() {
                        request = request.with_identity(sub_identity.clone());
                    }
                    if let Some(ref op_name) = sub_operation_name {
                        request = request.with_operation_name(op_name.clone());
                    }
                    if let Some(ref vars) = sub_variables {
                        request = request.with_variables(vars.clone());
                    }

                    // The request carries its context, so it runs on this
                    // service's own task.
                    let response = query_runner.execute(request).await;

                    // Skip empty results (filter excluded the document)
                    if !crate::subscription::response_has_data(&response) {
                        continue;
                    }

                    if let Ok(json) = serde_json::to_string(&response) {
                        result_tx.send_async(json).await;
                    }
                }
            }
        });

        let state = GraphQLSubscriptionState {
            result_receiver: result_rx,
            node_handle: node_ptr,
            event_sub_id: sub_id,
            service,
        };

        let handle = GRAPHQL_SUBSCRIPTIONS.insert(state);
        return Err(FfiResult::subscription(handle.to_string()));
    }

    // Check if this is an encrypted query (encrypted_<Collection>) and validate P2P is enabled
    // Go only generates the encrypted_<Collection> GraphQL field when P2P is enabled,
    // so we need to return a schema validation error if P2P is disabled
    if query_str.contains("encrypted_") {
        let has_p2p = NODES
            .get(node_ptr, |state| state.p2p.is_some())
            .unwrap_or(false);
        if !has_p2p {
            // Extract the collection name from the query to generate Go-compatible error
            // e.g., "encrypted_User" -> "Cannot query field \"encrypted_User\" on type \"Query\"."
            if let Some(start) = query_str.find("encrypted_") {
                let rest = &query_str[start..];
                // Find the end of the field name (next non-alphanumeric/underscore)
                let end = rest
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                let field_name = &rest[..end];
                return Err(FfiResult::error(format!(
                    "Cannot query field \"{}\" on type \"Query\".",
                    field_name
                )));
            }
            return Err(FfiResult::error(
                "Cannot query encrypted fields when P2P is disabled.",
            ));
        }
    }

    let runner = get_node_runner(node_ptr)?;
    let mut request = query::QueryRequest::new(query_str).with_context(context);
    if did.is_some() {
        request = request.with_identity(did);
    }
    if let Some(op) = op_name {
        request = request.with_operation_name(op);
    }
    if let Some(vars) = vars_str {
        let vars_json: serde_json::Value = serde_json::from_str(&vars)
            .map_err(|e| FfiResult::error(format!("failed to parse variables: {}", e)))?;
        request = request.with_variables(vars_json);
    }
    let handle = txn_id.map(|id| query::txn::TransactionHandle::new(id.to_string()));
    Ok((runner, request, handle))
}
