//! The reader a created document's genesis asks for, on a provider that
//! cannot join the create's transaction.
//!
//! A local provider grants this reader on the receiving node, which owns its
//! replica there (`merge_handler::genesis_access::register_signed_genesis`).
//! On a remote provider every node of the deployment shares one record of
//! the document and its creator owns it, so a receiver cannot grant itself:
//! the creator owes the grant from the create's own transaction, behind the
//! document's owed registration, and the receivers' strict merge gate lets
//! the document in once their provider holds both. The model is
//! `proofs/tla/RemoteGenesisReader.tla`; the choice of reader is
//! `creatorReader` in `proofs/lean/EnrollmentGenesis.lean`.

use std::sync::Arc;

use acp::Subject;
use datastore::NamespaceView;
use defra_core::RequestContext;
use document::Document;
use identity::Did;
use storage::corekv::OnCommit;

use crate::collection::Collection;
use crate::merge::merge_handler::genesis_access::{creator_reader, GENESIS_READER_RELATION};

/// Stage, as `creator` and in the create's transaction, the reader `doc`'s
/// genesis asks for, when its collection opted in. `datastore` is the
/// transaction's document view, which carries the host's opt-in and the
/// transaction's count of staged changes; its ACP sibling already holds the
/// document's owed registration.
pub(crate) async fn stage_genesis_reader(
    provider: &Arc<dyn acp::DocumentACP>,
    context: &RequestContext,
    datastore: &NamespaceView,
    collection: &Collection,
    doc: &Document,
    creator: &Did,
    doc_id: &str,
) -> query::error::Result<()> {
    let Some(policy) = collection.schema().policy.as_ref() else {
        return Ok(());
    };
    let access =
        datastore.genesis_read_access(collection.name(), &policy.id, &policy.resource_name);
    let immutable = |field: &str| {
        collection
            .schema()
            .fields
            .iter()
            .any(|declared| declared.name == field && declared.immutable)
    };
    let Some(reader) = creator_reader(access, |field| {
        immutable(field)
            .then(|| doc.get(field).and_then(document::NormalValue::as_str))
            .flatten()
    }) else {
        return Ok(());
    };
    let subject = if reader.is_wildcard() {
        Subject::Wildcard
    } else {
        Subject::Entity(reader)
    };
    let staged = datastore
        .projection_staged()
        .map(acp::StagedCount::from_shared)
        .unwrap_or_default();
    let first = staged.get() == 0;
    let acpstore = datastore.sibling(storage::namespace::Namespace::Acpstore);
    let outbox = acp::bound_to_request(provider, context)
        .transaction_outbox(&acpstore, staged)
        .ok_or_else(|| {
            query::error::QueryError::execution(format!(
                "collection {} asks its genesis to grant a reader, and its document access \
                 provider keeps no outbox to owe the grant in",
                collection.name()
            ))
        })?;
    let txn = context.txn().map(|txn| txn.0);
    outbox
        .stage(acp::StageRelationship {
            txn,
            actor: creator,
            subject,
            policy_id: &policy.id,
            resource: &policy.resource_name,
            object: doc_id,
            relation: GENESIS_READER_RELATION,
            managing_relations: &[],
            op: acp::OwedOp::Add,
        })
        .await
        .map_err(|error| {
            query::error::QueryError::execution(format!(
                "the reader document {doc_id}'s genesis asks for could not be owed: {error}"
            ))
        })?;
    if first {
        settle_after_commit(provider, datastore, txn).await?;
    }
    Ok(())
}

/// Once the transaction commits, send what it owes: the provider takes the
/// document's registration first, as every owed change waits behind its
/// object's registration. A failure is reported and the changes stay owed
/// for the next pass.
async fn settle_after_commit(
    provider: &Arc<dyn acp::DocumentACP>,
    view: &NamespaceView,
    txn: Option<u64>,
) -> query::error::Result<()> {
    let provider = Arc::clone(provider);
    view.on_commit(OnCommit::Async(Box::new(move || {
        Box::pin(async move {
            let scope = txn.map_or(acp::OwedScope::All, acp::OwedScope::Transaction);
            match workers::timeout(
                std::time::Duration::from_secs(10),
                provider.settle_owed_changes(scope),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    tracing::warn!(%error, "Owed genesis reader not settled; it stays owed");
                }
                Err(error) => {
                    tracing::warn!(%error, "Owed genesis reader settlement did not finish; it stays owed");
                }
            }
        })
    })))
    .await
    .map_err(|error| query::error::QueryError::execution(error.to_string()))
}
