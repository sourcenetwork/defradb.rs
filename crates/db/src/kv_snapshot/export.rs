//! Exporting documents as a KV snapshot in Go Shinzo's layout
//! (`internal/db/kv_export.go`), synthesized from defradb.rs's own state:
//! for each document its datastore entries, then its heads, then its blocks.

use std::io::Write;

use super::go_keys::{self, Instance};
use cid::Cid;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use storage::corekv::{IterOptions, Store};
use storage::keys::doc_id_index::decode_doc_short_id;

use super::error::KvSnapshotError;
use super::frame::{write_end, write_entry, BLOCKSTORE, DATASTORE, HEADSTORE};
use super::mapping::{field_entries, DocumentEntry, FieldMapping};
use crate::collection::Collection;
use crate::txn::DbTxn;

/// What an export writes besides each document's current values and heads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions {
    /// Only the datastore entries: no heads, no blocks (Go's `datastoreOnly`).
    pub datastore_only: bool,
    /// Every earlier version and signature block too, so another node can
    /// replay the documents' history. Without it the blocks are Go's: each
    /// head and the blocks it links, nothing older.
    pub history: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            datastore_only: false,
            history: true,
        }
    }
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    /// Frames written, not counting the end mark.
    pub frames: usize,
    /// The field mapping an importing node reads the snapshot with.
    pub mapping: FieldMapping,
}

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// The field mapping a KV snapshot of `collection` is read with (Go
    /// Shinzo's `ExportFieldMapping`), without exporting anything.
    pub fn kv_field_mapping(&self, collection: &str) -> Result<FieldMapping, KvSnapshotError> {
        if collection.is_empty() {
            return Err(KvSnapshotError::CollectionNameEmpty);
        }
        let collection = self
            .get_collection(collection)?
            .ok_or_else(|| KvSnapshotError::CollectionNotFound(collection.to_owned()))?;
        Ok(FieldMapping {
            collection_name: collection.name().to_owned(),
            collection_short_id: collection.resolved_root_id(),
            fields: field_entries(collection.schema()),
            documents: Vec::new(),
        })
    }

    /// Write `doc_ids` of `collection` to `out` as a KV snapshot, reading as
    /// the identity acting in `context`: a document it cannot read is left
    /// out, as one this node does not have is. No DocIDs exports every
    /// document of the collection. One read snapshot; nothing is written.
    pub async fn export_kv_snapshot_as<W>(
        &self,
        context: &defra_core::RequestContext,
        collection: &str,
        doc_ids: &[String],
        options: ExportOptions,
        out: &mut W,
    ) -> Result<Exported, KvSnapshotError>
    where
        W: Write + ?Sized,
    {
        if collection.is_empty() {
            return Err(KvSnapshotError::CollectionNameEmpty);
        }
        let collection = self
            .get_collection(collection)?
            .ok_or_else(|| KvSnapshotError::CollectionNotFound(collection.to_owned()))?;
        let txn = self.new_txn(true).await?;
        let exported = self
            .export_in(&txn, context, &collection, doc_ids, options, out)
            .await;
        if let Err(error) = txn.discard() {
            tracing::warn!(%error, "failed to discard a KV snapshot export transaction");
        }
        exported
    }

    async fn export_in<W>(
        &self,
        txn: &DbTxn<S>,
        context: &defra_core::RequestContext,
        collection: &Collection,
        doc_ids: &[String],
        options: ExportOptions,
        out: &mut W,
    ) -> Result<Exported, KvSnapshotError>
    where
        W: Write + ?Sized,
    {
        self.check_node_access_in_txn(
            context,
            txn.acpstore()?,
            acp::nac::NodePermission::DocumentRead,
        )
        .await?;
        let datastore = txn.datastore()?;
        let systemstore = txn.systemstore()?;
        let collection_short_id = collection.resolved_root_id();
        let fields = field_entries(collection.schema());
        let short_ids: RapidHashMap<&str, String> = fields
            .iter()
            .map(|field| (field.name.as_str(), field.short_id.to_string()))
            .collect();

        let documents = match doc_ids.is_empty() {
            true => self.every_document(txn, collection).await?,
            false => {
                let mut documents = Vec::with_capacity(doc_ids.len());
                for doc_id in doc_ids {
                    if let Some(doc_short_id) = crate::docid::map::get_doc_short_id(
                        &systemstore,
                        collection_short_id,
                        doc_id,
                    )
                    .await?
                    {
                        documents.push((doc_short_id, doc_id.clone()));
                    }
                }
                documents
            }
        };

        let mut frames = 0;
        let mut exported = Vec::with_capacity(documents.len());
        let mut slice = workers::Budget::new(16);
        for (doc_short_id, doc_id) in documents {
            slice.spend(1).await;
            if !self.readable(txn, context, collection, &doc_id).await?
                || datastore
                    .has(&collection.deleted_key(doc_short_id))
                    .await
                    .map_err(crate::Error::Storage)?
            {
                continue;
            }
            let Some(written) = self
                .export_document(txn, collection, &short_ids, doc_short_id, options, out)
                .await?
            else {
                continue;
            };
            frames += written;
            exported.push(DocumentEntry {
                doc_id,
                short_id: doc_short_id,
            });
        }
        write_end(out)?;
        Ok(Exported {
            frames,
            mapping: FieldMapping {
                collection_name: collection.name().to_owned(),
                collection_short_id,
                fields,
                documents: exported,
            },
        })
    }

    /// Every live document of `collection`, in short-ID order.
    async fn every_document(
        &self,
        txn: &DbTxn<S>,
        collection: &Collection,
    ) -> Result<Vec<(u64, String)>, KvSnapshotError> {
        let datastore = txn.datastore()?;
        let systemstore = txn.systemstore()?;
        let prefix = collection.collection_key_prefix();
        let mut iter = datastore
            .iterator(
                IterOptions::new()
                    .with_prefix(prefix.clone())
                    .with_keys_only(true),
            )
            .await
            .map_err(crate::Error::Storage)?;
        let mut short_ids = Vec::new();
        while let Some(pair) = iter.next().await.map_err(crate::Error::Storage)? {
            if let Ok(doc_short_id) = decode_doc_short_id(&pair.key[prefix.len()..]) {
                short_ids.push(doc_short_id);
            }
        }
        iter.close().await.map_err(crate::Error::Storage)?;
        let mut documents = Vec::with_capacity(short_ids.len());
        for doc_short_id in short_ids {
            if let Some(doc_id) = crate::docid::map::get_doc_id(&systemstore, doc_short_id).await? {
                documents.push((doc_short_id, doc_id));
            }
        }
        Ok(documents)
    }

    /// Whether the identity acting in `context` reads `doc_id`, decided at
    /// the export's snapshot as a query decides it.
    async fn readable(
        &self,
        txn: &DbTxn<S>,
        context: &defra_core::RequestContext,
        collection: &Collection,
        doc_id: &str,
    ) -> Result<bool, KvSnapshotError> {
        let (Some(policy), Some(provider)) = (&collection.schema().policy, self.document_acp())
        else {
            return Ok(true);
        };
        let provider = provider
            .bind_transaction(txn.acpstore()?)
            .unwrap_or_else(|| provider.clone());
        let identity = acp::Identity::from(
            context
                .identity()
                .and_then(|did| identity::Did::new(did).ok()),
        );
        Ok(query::txn::check_doc_read_access_as(
            context,
            provider.as_ref(),
            None,
            &identity,
            &policy.id,
            &policy.resource_name,
            collection.collection_id(),
            collection.schema().is_branchable,
            doc_id,
            None,
        )
        .await?)
    }

    /// One document's frames; `None` when it has no stored value.
    async fn export_document<W>(
        &self,
        txn: &DbTxn<S>,
        collection: &Collection,
        short_ids: &RapidHashMap<&str, String>,
        doc_short_id: u64,
        options: ExportOptions,
        out: &mut W,
    ) -> Result<Option<usize>, KvSnapshotError>
    where
        W: Write + ?Sized,
    {
        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let Some(stored) = datastore
            .get(&collection.doc_key(doc_short_id))
            .await
            .map_err(crate::Error::Storage)?
        else {
            return Ok(None);
        };
        let document = crdt::stored::document_from_storage(
            &stored,
            collection.schema_arc(),
            collection.crdts(),
        )?;
        let set = crate::block::heads::document::load(&headstore, doc_short_id)
            .await
            .map_err(crate::Error::Storage)?;

        // Each head as Go keys it, by field short ID, with its priority.
        let mut heads = Vec::new();
        let mut priorities: RapidHashMap<String, u64> = RapidHashMap::new();
        for (name, priority) in set.live() {
            let Some((field, cid)) = crate::block::heads::document::parse_name(name) else {
                continue;
            };
            let short_id = match field {
                "C" => "C".to_owned(),
                field => match short_ids.get(field) {
                    Some(short_id) => short_id.clone(),
                    None => continue,
                },
            };
            if short_id != "C" {
                let entry = priorities.entry(field.to_owned()).or_default();
                *entry = (*entry).max(priority);
            }
            heads.push((
                go_keys::headstore_key(doc_short_id, &short_id, &cid),
                cid,
                priority,
            ));
        }
        heads.sort_by(|left, right| left.0.cmp(&right.0));

        let collection_short_id = collection.resolved_root_id();
        let mut values = Vec::new();
        let mut priority_entries = Vec::new();
        for (field, priority) in &priorities {
            let (Some(value), Some(short_id)) = (document.get(field), short_ids.get(&field[..]))
            else {
                continue;
            };
            let value = crate::block::builder::encode_value_as_cbor(value)
                .map_err(KvSnapshotError::Malformed)?;
            values.push((
                go_keys::datastore_key(
                    collection_short_id,
                    Instance::Value,
                    doc_short_id,
                    short_id,
                ),
                value,
            ));
            priority_entries.push((
                go_keys::datastore_key(
                    collection_short_id,
                    Instance::Priority,
                    doc_short_id,
                    short_id,
                ),
                crate::block::builder::encode_priority_varint(*priority),
            ));
        }
        if let Some(version) = collection.load_version(&datastore, doc_short_id).await? {
            values.push((
                go_keys::datastore_key(collection_short_id, Instance::Value, doc_short_id, "v"),
                version.into_bytes(),
            ));
        }
        values.sort();
        priority_entries.sort();

        let mut frames = 0;
        for (key, value) in values.iter().chain(&priority_entries) {
            write_entry(out, DATASTORE, key, value)?;
            frames += 1;
        }
        if options.datastore_only {
            return Ok(Some(frames));
        }
        for (key, _, priority) in &heads {
            write_entry(
                out,
                HEADSTORE,
                key,
                &crate::block::builder::encode_priority_varint(*priority),
            )?;
            frames += 1;
        }
        let blockstore = txn.blockstore()?;
        let mut visited = RapidHashSet::new();
        for (_, head, _) in &heads {
            let mut stack = vec![*head];
            while let Some(cid) = stack.pop() {
                if !visited.insert(cid) {
                    continue;
                }
                let bytes = blockstore
                    .get(&cid.to_bytes())
                    .await
                    .map_err(crate::Error::Storage)?
                    .ok_or_else(|| {
                        KvSnapshotError::Malformed(format!("block {cid} is not stored"))
                    })?;
                write_entry(out, BLOCKSTORE, &cid.to_bytes(), &bytes)?;
                frames += 1;
                let Ok(block) = defra_core::block::Block::from_dag_cbor(&bytes) else {
                    continue;
                };
                let mut next: Vec<Cid> =
                    block.links.iter().flatten().map(|link| link.link).collect();
                if options.history {
                    next.extend(block.heads.iter().flatten().copied());
                    next.extend(block.signature);
                    next.extend(block.encryption);
                }
                stack.extend(next.into_iter().rev());
            }
        }
        Ok(Some(frames))
    }
}
