//! Unique index conflicts: healing an entry left by a deleted document, and
//! the deterministic winner a merge or a rebuild gives a value two live
//! documents hold. Every path that saves a unique entry outside a plain local
//! write goes through here, so the rule exists once.

use datastore::NamespaceView;
use rapidhash::{RapidHashMap, RapidHashSet};
use storage::corekv::Key;

use super::{IndexManager, IndexType};
use crate::index::error::{Error, Result};

impl IndexManager {
    /// Save an index entry, healing stale unique conflicts (#1111/#700).
    ///
    /// A unique violation whose existing entry points at a **deleted or
    /// missing** document is not a real conflict — it is damage from an era
    /// when merge/delete paths did not maintain indexes (four production
    /// stores were wounded this way, and such an entry otherwise blocks the
    /// value forever with no repair affordance). Deletion is terminal in
    /// DefraDB, so the stale holder can never become live again: reclaim the
    /// entry and proceed.
    ///
    /// A violation whose holder is alive is a genuine conflict and propagates
    /// unchanged — local-write semantics stay strict (Go parity).
    pub(super) async fn save_healing_stale_unique(
        &self,
        datastore: &mut NamespaceView,
        index: &IndexType,
        doc_short_id: u64,
        values: &[document::NormalValue],
    ) -> std::result::Result<(), storage::corekv::Error> {
        match index.save(datastore, doc_short_id, values).await {
            Err(storage::corekv::Error::UniqueConstraintViolation) => {
                let IndexType::Unique(unique) = index else {
                    return Err(storage::corekv::Error::UniqueConstraintViolation);
                };
                if self.collection_id.is_empty() {
                    return Err(storage::corekv::Error::UniqueConstraintViolation);
                }
                let Some(holder) = unique.conflicting_doc_id(datastore, values).await? else {
                    return Err(storage::corekv::Error::UniqueConstraintViolation);
                };
                if holder == doc_short_id {
                    // Re-indexing the same document with the same values is
                    // idempotent, not a conflict (content-addressed docIDs make
                    // identical-content recreates land on the same id).
                    return unique.save_blind(datastore, doc_short_id, values).await;
                }
                if doc_is_live(datastore, &self.collection_id, holder).await? {
                    return Err(storage::corekv::Error::UniqueConstraintViolation);
                }
                tracing::info!(
                    collection_id = %self.collection_id,
                    index = %index.description().name,
                    stale_doc_short_id = holder,
                    new_doc_short_id = doc_short_id,
                    "reclaimed stale unique index entry pointing at a deleted or missing document"
                );
                unique.save_blind(datastore, doc_short_id, values).await
            }
            other => other,
        }
    }

    /// Save an index entry while resolving live unique conflicts deterministically (#1111/#1308).
    ///
    /// A CRDT merge cannot preserve cross-replica uniqueness: two nodes that
    /// each locally accepted the same unique value must still converge when
    /// they sync. Failing the merge (the previous behavior, and Go's current
    /// behavior) wedges the document's entire forward history on both sides —
    /// permanent retry, permanent divergence. Instead, both documents persist
    /// and the unique entry goes to a deterministic winner, so every replica
    /// converges to the same index no matter the merge order:
    ///
    /// **the lexicographically smallest docID wins.**
    ///
    /// docIDs are immutable and totally ordered, so the pick is stable across
    /// time and identical on every replica — no version/height state needed.
    /// The losing document remains fully readable by non-index (scan) queries
    /// and is reported loudly; applications keep their own reconcile policy
    /// (source-inc/gents#694 already does).
    pub(super) async fn save_resolving_unique_conflict(
        &self,
        datastore: &mut NamespaceView,
        systemstore: &NamespaceView,
        index: &IndexType,
        doc_short_id: u64,
        doc_id: &str,
        values: &[document::NormalValue],
    ) -> std::result::Result<Option<events::UniqueIndexConflictData>, storage::corekv::Error> {
        match self
            .save_healing_stale_unique(datastore, index, doc_short_id, values)
            .await
        {
            Ok(()) => Ok(None),
            Err(storage::corekv::Error::UniqueConstraintViolation) => {
                let IndexType::Unique(unique) = index else {
                    return Err(storage::corekv::Error::UniqueConstraintViolation);
                };
                let Some(holder) = unique.conflicting_doc_id(datastore, values).await? else {
                    return Err(storage::corekv::Error::UniqueConstraintViolation);
                };
                // The deterministic winner is decided on the PUBLIC DocID, which
                // is byte-identical on every replica. The holder is stored as a
                // node-local short id (different per node), so it must be resolved
                // back to its public DocID before comparison — comparing short ids
                // would let replicas pick different winners and silently diverge.
                let Some(holder_doc_id) = holder_public_doc_id(systemstore, holder).await? else {
                    return Err(storage::corekv::Error::UniqueConstraintViolation);
                };
                if doc_id < holder_doc_id.as_str() {
                    unique.save_blind(datastore, doc_short_id, values).await?;
                    tracing::error!(
                        collection_id = %self.collection_id,
                        index = %index.description().name,
                        winner_doc_id = %doc_id,
                        unindexed_doc_id = %holder_doc_id,
                        "unique index conflict: incoming document wins the \
                         deterministic pick; the previous holder is no longer indexed"
                    );
                    Ok(Some(events::UniqueIndexConflictData {
                        collection_id: self.collection_id.to_string(),
                        index: index.description().name.clone(),
                        winner_doc_id: doc_id.to_string(),
                        unindexed_doc_id: holder_doc_id.to_string(),
                    }))
                } else {
                    tracing::error!(
                        collection_id = %self.collection_id,
                        index = %index.description().name,
                        winner_doc_id = %holder_doc_id,
                        unindexed_doc_id = %doc_id,
                        "unique index conflict: existing holder wins the \
                         deterministic pick; the incoming document is persisted unindexed"
                    );
                    Ok(Some(events::UniqueIndexConflictData {
                        collection_id: self.collection_id.to_string(),
                        index: index.description().name.clone(),
                        winner_doc_id: holder_doc_id.to_string(),
                        unindexed_doc_id: doc_id.to_string(),
                    }))
                }
            }
            Err(other) => Err(other),
        }
    }

    /// Save one unique value set during a rebuild, after the entries were
    /// cleared. A value another live document holds goes to the merge's
    /// deterministic winner when both documents held it before the migration
    /// the rebuild applies, and is refused when that migration gave it to
    /// either of them. `original_keys` holds, for each document the rebuild
    /// migrated so far in this transaction, its unique keys from before; a
    /// document missing from it ran no transform.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn save_rebuilt_unique(
        &self,
        datastore: &mut NamespaceView,
        systemstore: &NamespaceView,
        index: &IndexType,
        unique: &storage::index::UniqueIndex,
        doc_short_id: u64,
        doc_id: &str,
        values: &[document::NormalValue],
        original_keys: &RapidHashMap<u64, RapidHashSet<Vec<u8>>>,
    ) -> Result<()> {
        // Conflict resolution is a read-modify-write on the shared
        // transaction, so the values go in one at a time: the winner then
        // depends on the public DocID ordering, never on scheduling.
        let holder = if original_keys.is_empty() {
            None
        } else {
            unique
                .conflicting_doc_id(datastore, values)
                .await
                .map_err(Error::Storage)?
        };
        if let Some(holder) = holder {
            let key = self.encode_index_key(index.description(), values)?;
            if holder != doc_short_id
                && [holder, doc_short_id].iter().any(|id| {
                    original_keys
                        .get(id)
                        .is_some_and(|keys| !keys.contains(&key))
                })
            {
                return Err(Error::Storage(
                    storage::corekv::Error::UniqueConstraintViolation,
                ));
            }
        }
        self.save_resolving_unique_conflict(
            datastore,
            systemstore,
            index,
            doc_short_id,
            doc_id,
            values,
        )
        .await
        .map_err(Error::Storage)?;
        Ok(())
    }
}

/// Whether the document a unique index entry points at is still alive: its
/// body exists and it carries no logical-deletion marker. Runs against the
/// same transactional view as the index write, so the answer is consistent
/// with the mutation being attempted.
async fn doc_is_live(
    datastore: &NamespaceView,
    collection_id: &str,
    doc_short_id: u64,
) -> std::result::Result<bool, storage::corekv::Error> {
    let body = storage::keys::doc_key(collection_id, doc_short_id);
    if !datastore.has(&body).await? {
        return Ok(false);
    }
    let deleted = storage::keys::deleted_doc_key(collection_id, doc_short_id);
    Ok(!datastore.has(&deleted).await?)
}

/// Resolve a unique entry holder's node-local short ID to its public DocID via
/// the systemstore doc-ID index.
///
/// The deterministic merge winner MUST be chosen on this public DocID, which is
/// byte-identical on every replica; the short id is node-local and differs per
/// node, so comparing short ids would break cross-replica convergence.
async fn holder_public_doc_id(
    systemstore: &NamespaceView,
    doc_short_id: u64,
) -> std::result::Result<Option<String>, storage::corekv::Error> {
    let key = storage::keys::DocShortIDToDocIDKey::new(doc_short_id).bytes();
    match systemstore.get(&key).await? {
        Some(bytes) => {
            Ok(Some(String::from_utf8(bytes.into()).map_err(|e| {
                storage::corekv::Error::Other(e.to_string())
            })?))
        }
        None => Ok(None),
    }
}
impl IndexManager {
    /// `error`, unless it is a unique violation of `index` by a local write:
    /// then the error naming `doc`, the writer, and the `values` it wrote,
    /// never the document holding them, which the writer may not be allowed
    /// to read (`defra.index.unique_writer`).
    pub(super) fn naming_the_writer(
        error: Error,
        doc: &document::Document,
        index: &IndexType,
        values: &[document::NormalValue],
    ) -> Error {
        let Some(doc_id) = doc.id().filter(|_| error.is_unique_constraint_violation()) else {
            return error;
        };
        let fields = index
            .description()
            .fields
            .iter()
            .zip(values)
            .map(|(field, value)| format!(", {}: {}", field.name, go_value(value)))
            .collect();
        Error::UniqueViolation {
            doc_id: doc_id.to_string(),
            fields,
        }
    }

    /// [`IndexManager::naming_the_writer`] for an update, which Go also wraps
    /// with the index's name.
    pub(super) fn naming_the_updater(
        error: Error,
        doc: &document::Document,
        index: &IndexType,
        values: &[document::NormalValue],
    ) -> Error {
        match Self::naming_the_writer(error, doc, index, values) {
            named @ Error::UniqueViolation { .. } => Error::UpdateIndex {
                index: index.description().name.clone(),
                source: Box::new(named),
            },
            other => other,
        }
    }
}

/// `value` as Go's `%v` prints it in an error. Scalars match Go's text, the
/// part clients match on; other kinds print their Debug form.
fn go_value(value: &document::NormalValue) -> String {
    use document::NormalValue::*;
    match value {
        value if value.is_nil() => "<nil>".to_string(),
        String(text) | NillableString(Some(text)) => text.clone(),
        Int(n) | NillableInt(Some(n)) => n.to_string(),
        Float64(n) | NillableFloat64(Some(n)) => n.to_string(),
        Bool(b) | NillableBool(Some(b)) => b.to_string(),
        other => format!("{other:?}"),
    }
}
