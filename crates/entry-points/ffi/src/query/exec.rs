use std::ffi::c_char;

use crate::call::{self, DefraResultCallback, Reply};
use crate::ffi_entry;
use crate::helpers::{get_node_runner, get_rt};
use crate::nac_check::admit_on_node;
use crate::state::{GraphQLSubscriptionState, GRAPHQL_SUBSCRIPTIONS, NODES};
use crate::target::{go_string, non_empty, resolve_caller, resolve_target, Caller, Target};
use crate::types::DefraResult;
use crate::{try_ffi, ERR_INVALID_NODE_HANDLE};

use super::{
    dac_bypass, nac_permission_for_query, subscription_accepts_doc_id, subscription_doc_ids,
    subscription_to_scoped_query,
};

/// Executes a GraphQL request on a node, or in a transaction, as the
/// identity `identity_ptr` names (0 for none).
///
/// A query or mutation answers with status 0 and its GraphQL result as JSON,
/// `{"data": ..., "errors": [...]}`; GraphQL errors are part of that result.
/// A subscription answers with status 2 and the subscription's ID, which
/// `defra_subscription_poll` reads and `defra_subscription_close` ends.
/// Status 1 reports a request that could not run.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated UTF-8 string;
/// `operation_name` and `variables` may be null or empty for none.
#[no_mangle]
pub unsafe extern "C" fn defra_query_execute(
    node_ptr: usize,
    query: *const c_char,
    identity_ptr: usize,
    operation_name: *const c_char,
    variables: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let args = try_ffi!(ExecArgs::read(node_ptr, query, identity_ptr, operation_name, variables));
        call::wait(async move {
            Ok(match prepare(args).await {
                Ok(prepared) => DefraResult::from(respond(prepared.run().await)),
                Err(decided) => decided,
            })
        })
        .unwrap_or_else(DefraResult::error)
    }
}

/// [`defra_query_execute`] for a caller that is called back instead of
/// waiting: returns once the request is admitted (to the node's worker pool,
/// or, in a transaction, to its turn on the transaction), and `callback`
/// receives `context` and the result exactly once, as [`defra_query_execute`]
/// would return it, on the DefraDB thread that ran the request. In a
/// transaction, calls made on it before this one run first and calls made
/// after it (a commit included) wait for it. A request refused before it
/// runs (an invalid argument, a denied permission) and a subscription, which
/// answers at once with its ID, call it on the caller's thread before this
/// returns.
///
/// # Safety
///
/// As for [`defra_query_execute`]; the strings are read only before this
/// returns.
#[no_mangle]
pub unsafe extern "C" fn defra_query_execute_async(
    node_ptr: usize,
    query: *const c_char,
    identity_ptr: usize,
    operation_name: *const c_char,
    variables: *const c_char,
    callback: DefraResultCallback,
    context: usize,
) {
    let reply = Reply::new(callback, context);
    // A panic drops `reply`, which reports the call ended without a result.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        match ExecArgs::read(node_ptr, query, identity_ptr, operation_name, variables) {
            Ok(args) => call::detach(async move {
                match prepare(args).await {
                    Ok(Prepared::Node { runner, request }) => {
                        let done = move |response| reply.send(DefraResult::from(respond(response)));
                        runner.execute_then(request, Box::new(done)).await
                    }
                    Ok(prepared) => run_in_turn(prepared, reply),
                    Err(decided) => reply.send(decided),
                }
            }),
            Err(decided) => reply.send(decided),
        }
    }));
}

/// The response's GraphQL JSON.
fn respond(response: query::QueryResponse) -> Result<String, String> {
    serde_json::to_string(&response).map_err(|e| format!("failed to serialize response: {}", e))
}

/// An execute call's arguments, read from the caller's strings and handles
/// before the call returns.
struct ExecArgs {
    target: Target,
    caller: Caller,
    query: String,
    operation_name: Option<String>,
    variables: Option<String>,
}

impl ExecArgs {
    /// # Safety
    ///
    /// As for [`defra_query_execute`].
    unsafe fn read(
        node_ptr: usize,
        query: *const c_char,
        identity_ptr: usize,
        operation_name: *const c_char,
        variables: *const c_char,
    ) -> Result<Self, DefraResult> {
        Ok(Self {
            target: resolve_target(node_ptr)?,
            caller: resolve_caller(identity_ptr)?,
            query: go_string(query),
            operation_name: non_empty(operation_name),
            variables: non_empty(variables),
        })
    }
}

/// A request ready to run, on its node or in its transaction.
enum Prepared {
    Node {
        runner: std::sync::Arc<dyn query::QueryExecutor>,
        request: query::QueryRequest,
    },
    Txn {
        runner: std::sync::Arc<dyn query::QueryExecutor>,
        request: query::QueryRequest,
        handle: query::txn::TransactionHandle,
        /// The call's turn on the transaction, held until the request ends.
        turn: Target,
    },
}

impl Prepared {
    /// Runs the request: in a transaction, once the calls made on it before
    /// this one have run.
    async fn run(self) -> query::QueryResponse {
        match self {
            Prepared::Node { runner, request } => runner.execute(request).await,
            Prepared::Txn {
                runner,
                request,
                handle,
                turn,
            } => {
                turn.turn().await;
                let response = runner.execute_in_txn(request, &handle).await;
                drop(turn);
                response
            }
        }
    }
}

/// Answers `reply` with a transaction request's result once it has run, on
/// the network runtime, so the caller returns as soon as the request holds
/// its turn: calls made on the transaction before it run first, calls made
/// after it wait for it.
fn run_in_turn(prepared: Prepared, reply: Reply) {
    match crate::helpers::get_rt() {
        // A task the runtime refuses drops `reply`, which reports that the
        // call ended without a result.
        Ok(rt) => drop(rt.spawn(async move {
            reply.send(DefraResult::from(respond(prepared.run().await)));
        })),
        Err(error) => reply.send(error),
    }
}

/// Checks an execute call and returns the request to run. `Err` is the
/// call's result when it is decided before any request runs: an error, or a
/// subscription's ID.
async fn prepare(args: ExecArgs) -> Result<Prepared, DefraResult> {
    let ExecArgs {
        target,
        caller,
        query: query_str,
        operation_name: op_name,
        variables: vars_str,
    } = args;
    let identity_str = caller.did.clone();
    let node_ptr = target.node();

    let permission = nac_permission_for_query(&query_str);
    admit_on_node(node_ptr, identity_str.as_deref(), permission).await?;

    let did = match identity_str {
        Some(ref s) => match identity::Did::new(s) {
            Ok(d) => Some(d),
            Err(e) => return Err(DefraResult::error(format!("invalid identity DID: {}", e))),
        },
        None => None,
    };

    // The signer for block signing during mutations. Matches Go's
    // behavior: if signing is enabled and no explicit identity, fall back
    // to node identity for block signing.
    let (node_did, signing_enabled) = NODES
        .get(node_ptr, |state| {
            (state.identity_did(), state.signing_enabled)
        })
        .unwrap_or((None, false));
    let signing = defra_core::signing::resolve_signing_config_with_flag(
        identity_str.as_deref(),
        node_did.as_deref(),
        signing_enabled,
    );
    let session_key = signing.as_ref().map(|s| s.public_key_hex.clone());

    // The caller acts in the request's context, so DB-layer NAC checks
    // resolve it; a NAC admin/owner can read all documents.
    let context = caller.present(
        defra_core::RequestContext::default()
            .with_txn(target.txn_id())
            .with_identity(identity_str.clone())
            .with_dac_bypass(dac_bypass(node_ptr, identity_str.as_deref()).await)
            .with_signing(signing)
            .with_batch_session(session_key),
    );

    let variables = match vars_str {
        Some(vars) => Some(
            serde_json::from_str::<serde_json::Value>(&vars)
                .map_err(|e| DefraResult::error(format!("failed to parse variables: {}", e)))?,
        ),
        None => None,
    };

    if query_str.trim_start().starts_with("subscription") {
        return Err(subscribe(
            node_ptr, query_str, did, op_name, variables, context,
        )?);
    }

    // Go only generates the encrypted_<Collection> GraphQL field when P2P is
    // enabled, so without P2P such a query fails schema validation.
    if query_str.contains("encrypted_") {
        let has_p2p = NODES
            .get(node_ptr, |state| state.p2p.is_some())
            .unwrap_or(false);
        if !has_p2p {
            if let Some(start) = query_str.find("encrypted_") {
                let rest = &query_str[start..];
                let end = rest
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                let field_name = &rest[..end];
                return Err(DefraResult::error(format!(
                    "Cannot query field \"{}\" on type \"Query\".",
                    field_name
                )));
            }
            return Err(DefraResult::error(
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
    if let Some(vars) = variables {
        request = request.with_variables(vars);
    }
    Ok(match target.txn_id() {
        None => Prepared::Node { runner, request },
        Some(id) => Prepared::Txn {
            runner,
            request,
            handle: query::txn::TransactionHandle::new(id.to_string()),
            turn: target,
        },
    })
}

/// Starts a GraphQL subscription and returns its status-2 result: the ID
/// `defra_subscription_poll` reads it by.
///
/// Subscription queries run again at event time, not poll time, so the
/// store state each result reflects is the event's: a service processes
/// events as they arrive, runs the query scoped to the changed document, and
/// buffers the GraphQL results for polling.
fn subscribe(
    node_ptr: usize,
    query_str: String,
    did: Option<identity::Did>,
    op_name: Option<String>,
    variables: Option<serde_json::Value>,
    context: defra_core::RequestContext,
) -> Result<DefraResult, DefraResult> {
    // Validate before subscribing. The per-event path below can only log a
    // document it will always reject, leaving the caller polling an ID that
    // never yields.
    query::subscription::validate_subscription(&query_str, op_name.as_deref())
        .map_err(|error| DefraResult::error(error.to_string()))?;

    let (mut subscription, query_runner) = NODES
        .get(node_ptr, |state| {
            (
                state.event_bus.subscribe(&[events::EventName::Update]),
                state.query_runner.clone(),
            )
        })
        .ok_or_else(|| DefraResult::error(ERR_INVALID_NODE_HANDLE))?;
    let sub_id = subscription.id();

    let doc_ids_filter = subscription_doc_ids(&query_str, variables.as_ref(), op_name.as_deref());
    let (result_tx, result_rx) = kovan_channel::bounded::<String>(256);

    let service = get_rt()?.spawn_service(async move {
        while let Some(message) = subscription.recv().await {
            let Some(update) = message.as_update() else {
                continue;
            };
            let event_doc_id = update.doc_id.clone();
            if !subscription_accepts_doc_id(doc_ids_filter.as_deref(), &event_doc_id) {
                continue;
            }

            let cid_str = update.cid.to_string();
            let query_text = match subscription_to_scoped_query(
                &query_str,
                &event_doc_id,
                &cid_str,
                op_name.as_deref(),
            ) {
                Ok(query) => query,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to scope subscription query");
                    continue;
                }
            };

            let mut request = query::QueryRequest::new(query_text).with_context(context.clone());
            if did.is_some() {
                request = request.with_identity(did.clone());
            }
            if let Some(ref op_name) = op_name {
                request = request.with_operation_name(op_name.clone());
            }
            if let Some(ref vars) = variables {
                request = request.with_variables(vars.clone());
            }

            // The request carries its context, so it runs on this service's
            // own task.
            let response = query_runner.execute(request).await;

            // A filter that excluded the document leaves no result.
            if !crate::subscription::response_has_data(&response) {
                continue;
            }

            if let Ok(json) = serde_json::to_string(&response) {
                result_tx.send_async(json).await;
            }
        }
    });

    let id = GRAPHQL_SUBSCRIPTIONS.insert(GraphQLSubscriptionState {
        result_receiver: result_rx,
        node_handle: node_ptr,
        event_sub_id: sub_id,
        service,
    });
    Ok(DefraResult::subscription(id.to_string()))
}
