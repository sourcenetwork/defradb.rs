use std::sync::Arc;

use async_lock::{Mutex, MutexGuard};
use async_trait::async_trait;
use cid::Cid;
use datastore::NamespaceView;
use defra_core::block::{Block, CrdtDelta};
use defra_core::thread_bounds::MaybeSendSync;
use document::{DocID, Document, NormalValue};
use rapidhash::RapidHashMap;
use storage::corekv::{IterOptions, Store};
use storage::index::SimpleIndex;
use storage::keys::doc_id_index::decode_doc_short_id;

use crate::collection::Collection;
use crate::merge::merge_handler::DbMergeHandler;
use crate::txn::DbTxn;

/// A field a composite links to, as its field block carries it.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Value(NormalValue),
    /// The field block is encrypted; its value is not a verdict input.
    Encrypted,
    /// This node does not hold the field block.
    NotHeld(Cid),
    /// The field block is held but its value does not decode.
    Undecodable,
}

/// The reads a [`super::MergeValidator`] may base a verdict on.
///
/// Every read is addressed by content, or scoped to one snapshot taken for the
/// verdict. `Ok(None)` means the input is not held: a validator defers on it,
/// naming the CID it waited for.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait MergeView: MaybeSendSync {
    async fn block(&self, cid: &Cid) -> Result<Option<Block>, String>;

    /// The fields a composite links to, by field name.
    async fn composite_fields(
        &self,
        cid: &Cid,
    ) -> Result<Option<Vec<(String, FieldValue)>>, String>;

    /// The genesis composite reached by following first heads from `cid`.
    /// `None` when an ancestor on the way is not held.
    async fn genesis(&self, cid: &Cid) -> Result<Option<Cid>, String>;

    /// Document IDs in `collection` whose merged `field` equals `value`, read
    /// from one snapshot shared by every call during this verdict.
    ///
    /// Only an `@immutable` scalar LWW field may be looked up; any other field
    /// is an `Err`. An immutable field is set once, so a document that matches
    /// on one replica matches on every replica holding it, whatever order its
    /// updates merged in. A mutable field could match on one replica and not
    /// another. Documents still arrive over time, so an empty result must
    /// defer, never reject.
    ///
    /// Deleted documents are included: a delete merges on one replica before
    /// another, and an immutable field does not change by being deleted, so
    /// presence here depends only on whether the document has replicated. The
    /// ids come back sorted by id string, so two replicas holding the same
    /// documents return the same `Vec`.
    async fn find_documents(
        &self,
        collection: &str,
        field: &str,
        value: &NormalValue,
    ) -> Result<Vec<String>, String>;

    /// The `@immutable` scalar LWW fields of the merged document `doc_id` in
    /// `collection`, read from the same snapshot as [`Self::find_documents`].
    /// Mutable fields are never returned: only a field set once reads the same
    /// on every replica that holds the document, whatever order its updates
    /// merged in.
    ///
    /// Absence is not stable, so it must defer, never reject: `None` means the
    /// document is not merged here yet, and an immutable field missing from
    /// the list may still be set by an update not yet merged. A deleted
    /// document is still returned: deletion is a merge like any other, and a
    /// read that changed with it would give two replicas different verdicts.
    async fn immutable_fields(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<Option<Vec<(String, NormalValue)>>, String>;
}

/// By (collection, field); `None` when the field has no index a lookup can use.
type ChosenIndexes = RapidHashMap<(String, String), Option<Arc<SimpleIndex>>>;

pub(crate) struct DbMergeView<'a, S: Store, B: blockstore::Blockstore> {
    handler: &'a DbMergeHandler<S, B>,
    snapshot: Mutex<Option<DbTxn<S>>>,
    /// A writing transaction's (datastore, systemstore) to read documents
    /// through instead of a snapshot: the local write path, where the
    /// documents this transaction has written must be visible. Indexes are
    /// not consulted then, since they do not see uncommitted writes.
    stores: Option<(&'a NamespaceView, &'a NamespaceView)>,
    /// The index chosen for each (collection, field) looked up this verdict.
    pub(super) indexes: Mutex<ChosenIndexes>,
    /// The deleted documents of each collection scanned this verdict, by
    /// collection name: the snapshot is fixed, so one scan serves every
    /// lookup the verdict makes.
    deleted: Mutex<RapidHashMap<String, Arc<Vec<Document>>>>,
}

impl<'a, S: Store, B: blockstore::Blockstore> DbMergeView<'a, S, B> {
    pub(crate) fn new(handler: &'a DbMergeHandler<S, B>) -> Self {
        Self {
            handler,
            snapshot: Mutex::new(None),
            stores: None,
            indexes: Mutex::new(RapidHashMap::default()),
            deleted: Mutex::new(RapidHashMap::default()),
        }
    }

    /// A view that reads documents through `datastore` and `systemstore`,
    /// a writing transaction's own, rather than through a snapshot.
    pub(crate) fn with_stores(
        handler: &'a DbMergeHandler<S, B>,
        datastore: &'a NamespaceView,
        systemstore: &'a NamespaceView,
    ) -> Self {
        Self {
            stores: Some((datastore, systemstore)),
            ..Self::new(handler)
        }
    }

    pub(crate) async fn finish(self) {
        if let Some(txn) = self.snapshot.into_inner() {
            let _ = txn.discard();
        }
    }

    async fn load(&self, cid: &Cid) -> Result<Option<Block>, String> {
        match self.handler.blockstore.get(cid).await {
            Ok(Some(data)) => Block::from_dag_cbor(&data)
                .map(Some)
                .map_err(|error| error.to_string()),
            Ok(None) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, B> MergeView for DbMergeView<'_, S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    async fn block(&self, cid: &Cid) -> Result<Option<Block>, String> {
        self.load(cid).await
    }

    async fn composite_fields(
        &self,
        cid: &Cid,
    ) -> Result<Option<Vec<(String, FieldValue)>>, String> {
        let Some(composite) = self.load(cid).await? else {
            return Ok(None);
        };
        let mut fields = Vec::new();
        for link in composite.links.iter().flatten() {
            let value = match self.load(&link.link).await? {
                None => FieldValue::NotHeld(link.link),
                Some(field) if field.encryption.is_some() => FieldValue::Encrypted,
                Some(field) => match &field.delta {
                    CrdtDelta::Lww(payload) => decode(&payload.data),
                    CrdtDelta::Counter(payload) => decode(&payload.data),
                    _ => FieldValue::Undecodable,
                },
            };
            fields.push((link.name.clone(), value));
        }
        Ok(Some(fields))
    }

    async fn genesis(&self, cid: &Cid) -> Result<Option<Cid>, String> {
        let mut current = *cid;
        for depth in 0.. {
            self.handler
                .ensure_merge_depth(&current, depth)
                .map_err(|error| error.to_string())?;
            let Some(block) = self.load(&current).await? else {
                return Ok(None);
            };
            match block.heads.as_deref().and_then(<[Cid]>::first) {
                Some(parent) => current = *parent,
                None => return Ok(Some(current)),
            }
        }
        unreachable!("the depth check ends the walk")
    }

    async fn find_documents(
        &self,
        collection: &str,
        field: &str,
        value: &NormalValue,
    ) -> Result<Vec<String>, String> {
        let Some(collection) = self.collection(collection)? else {
            return Ok(Vec::new());
        };
        if !super::is_immutable_scalar_field(collection.schema(), field) {
            return Err(format!(
                "find_documents field '{field}' in collection '{}' must be an @immutable scalar LWW field",
                collection.name()
            ));
        }
        let indexed = if self.stores.is_some() {
            None
        } else {
            self.indexed_documents(&collection, field, value).await?
        };
        let documents = match indexed {
            Some(documents) => documents,
            None => self.documents(&collection).await?,
        };
        let mut ids: Vec<String> = documents
            .into_iter()
            .filter(|document| document.get(field) == Some(value))
            .filter_map(|document| document.id().map(|id| id.to_string()))
            .collect();
        // Both paths yield node-local short-id order; the id string is the
        // same on every replica.
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    async fn immutable_fields(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<Option<Vec<(String, NormalValue)>>, String> {
        let Some((collection, document)) = self.document(collection, doc_id).await? else {
            return Ok(None);
        };
        let schema = collection.schema();
        let mut fields: Vec<(String, NormalValue)> = document
            .field_names()
            .filter(|name| super::is_immutable_scalar_field(schema, name))
            .filter_map(|name| {
                document
                    .get(name)
                    .map(|value| (name.to_string(), value.clone()))
            })
            .collect();
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Some(fields))
    }
}

impl<S: Store, B: blockstore::Blockstore> DbMergeView<'_, S, B> {
    /// The verdict's snapshot, opened on first use.
    pub(super) async fn snapshot(&self) -> Result<MutexGuard<'_, Option<DbTxn<S>>>, String> {
        let mut snapshot = self.snapshot.lock().await;
        if snapshot.is_none() {
            let txn = self.handler.db.new_txn(true).await;
            *snapshot = Some(txn.map_err(|error| error.to_string())?);
        }
        Ok(snapshot)
    }

    fn collection(&self, name: &str) -> Result<Option<Collection>, String> {
        self.handler
            .db
            .get_collection(name)
            .map_err(|error| error.to_string())
    }

    /// The merged document `doc_id` of `collection`, read by id, deleted or
    /// not. `None` when the collection or the document is unknown.
    async fn document(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<Option<(Collection, Document)>, String> {
        let Some(collection) = self.collection(collection)? else {
            return Ok(None);
        };
        let Ok(parsed) = doc_id.parse::<DocID>() else {
            return Ok(None);
        };
        let snapshot;
        let owned_datastore;
        let owned_systemstore;
        let (datastore, systemstore): (&NamespaceView, &NamespaceView) = match self.stores {
            Some(stores) => stores,
            None => {
                snapshot = self.snapshot().await?;
                let txn = snapshot.as_ref().expect("snapshot opened above");
                owned_datastore = txn.datastore().map_err(|error| error.to_string())?;
                owned_systemstore = txn.systemstore().map_err(|error| error.to_string())?;
                (&owned_datastore, &owned_systemstore)
            }
        };
        let Some((short_id, canonical)) = collection
            .resolve_doc_identity(systemstore, &parsed)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        // An alias resolves to its document; only the canonical id names it here.
        if canonical.to_string() != doc_id {
            return Ok(None);
        }
        let document = collection
            .get_with_datastore_include_deleted(datastore, short_id, &canonical, false)
            .await
            .map_err(|error| error.to_string())?
            .map(|(document, _)| document);
        Ok(document.map(|document| (collection, document)))
    }

    /// Every merged document of `collection` on this verdict's snapshot,
    /// deleted or not.
    async fn documents(&self, collection: &Collection) -> Result<Vec<Document>, String> {
        let snapshot;
        let owned_datastore;
        let owned_systemstore;
        let (datastore, systemstore): (&NamespaceView, &NamespaceView) = match self.stores {
            Some(stores) => stores,
            None => {
                snapshot = self.snapshot().await?;
                let txn = snapshot.as_ref().expect("snapshot opened above");
                owned_datastore = txn.datastore().map_err(|error| error.to_string())?;
                owned_systemstore = txn.systemstore().map_err(|error| error.to_string())?;
                (&owned_datastore, &owned_systemstore)
            }
        };
        Ok(collection
            .get_all_with_datastore_include_deleted(datastore, systemstore, true)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(document, _)| document)
            .collect())
    }

    /// The deleted documents of `collection` on this verdict's snapshot: a
    /// scan of the deletion markers, whose cost follows the deleted rows and
    /// not the collection. An index drops a document's entries when it is
    /// deleted, so a lookup through an index has to add these back. Deletion
    /// markers are never removed, so the scan is done once per collection
    /// per verdict, however many lookups the verdict makes.
    pub(super) async fn deleted_documents(
        &self,
        collection: &Collection,
    ) -> Result<Arc<Vec<Document>>, String> {
        if let Some(cached) = self.deleted.lock().await.get(collection.name()).cloned() {
            return Ok(cached);
        }
        let documents = Arc::new(self.scan_deleted_documents(collection).await?);
        self.deleted
            .lock()
            .await
            .insert(collection.name().to_string(), documents.clone());
        Ok(documents)
    }

    async fn scan_deleted_documents(
        &self,
        collection: &Collection,
    ) -> Result<Vec<Document>, String> {
        let snapshot = self.snapshot().await?;
        let txn = snapshot.as_ref().expect("snapshot opened above");
        let datastore = txn.datastore().map_err(|error| error.to_string())?;
        let systemstore = txn.systemstore().map_err(|error| error.to_string())?;

        let mut prefix = storage::keys::document::DELETED_KEY_PREFIX.to_vec();
        prefix.extend_from_slice(collection.collection_id().as_bytes());
        prefix.push(b'/');
        let prefix_len = prefix.len();
        let mut markers = datastore
            .iterator(IterOptions::new().with_prefix(prefix))
            .await
            .map_err(|error| error.to_string())?;
        let mut short_ids = Vec::new();
        while let Some(pair) = markers.next().await.map_err(|error| error.to_string())? {
            if let Ok(short_id) = decode_doc_short_id(&pair.key[prefix_len..]) {
                short_ids.push(short_id);
            }
        }
        markers.close().await.map_err(|error| error.to_string())?;
        if short_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(collection
            .get_by_short_ids(&datastore, &systemstore, &short_ids, true)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(_, document, _)| document)
            .collect())
    }
}

pub(super) fn decode(data: &[u8]) -> FieldValue {
    ciborium::from_reader::<NormalValue, _>(data)
        .map(FieldValue::Value)
        .unwrap_or(FieldValue::Undecodable)
}
