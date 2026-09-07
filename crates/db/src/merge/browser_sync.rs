use bytes::Bytes;
use std::collections::HashSet;
use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use defra_core::browser_sync::{BrowserSyncBlock, BrowserSyncDocument, MAX_SYNC_PAYLOAD_BYTES};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use storage::corekv::{IterOptions, Key as _, Store};
use storage::keys::BrowserSyncHeadKey;

use crate::event::emission::{TxnBroadcastEvent, TxnBroadcaster};
use crate::merge::merge_handler::DbMergeHandler;
use crate::merge::push_docs_common::{load_latest_composite_head_cids, load_push_dag_blocks};

mod validation;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BrowserSyncError {
    #[error("invalid sync document: {0}")]
    Invalid(String),
    #[error("sync document is too large: {0}")]
    TooLarge(String),
    #[error("sync storage failed: {0}")]
    Storage(String),
    #[error("sync merge failed: {0}")]
    Merge(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSyncDocumentRef {
    pub doc_id: String,
    pub collection_id: String,
    doc_short_id: u64,
}

impl BrowserSyncDocumentRef {
    /// Node-local short id of the referenced document.
    pub fn doc_short_id(&self) -> u64 {
        self.doc_short_id
    }
}

pub struct ValidatedBrowserSyncDocument {
    doc_id: String,
    collection_id: String,
    roots: Vec<Cid>,
    blocks: Vec<(Cid, Bytes)>,
    verified_genesis_creator: Option<String>,
}

impl ValidatedBrowserSyncDocument {
    pub fn doc_id(&self) -> &str {
        &self.doc_id
    }

    pub fn collection_id(&self) -> &str {
        &self.collection_id
    }

    /// The DID cryptographically verified from the genesis block's signature,
    /// if the genesis block is signed. This is the only identity that may be
    /// registered as the document's ACP owner.
    pub fn verified_genesis_creator(&self) -> Option<&str> {
        self.verified_genesis_creator.as_deref()
    }
}

pub struct BrowserSyncEngine<S: Store + 'static> {
    db: Arc<crate::DB<S>>,
    blockstore: Arc<DefraBlockstore<S>>,
    merge_handler: Arc<DbMergeHandler<S, DefraBlockstore<S>>>,
    broadcaster: Option<Arc<dyn TxnBroadcaster>>,
}

impl<S: Store + 'static> BrowserSyncEngine<S> {
    pub fn new(db: Arc<crate::DB<S>>) -> Self {
        Self::build(db, None)
    }

    /// Use this when running with the P2P stack, so that a fragment pushed to
    /// `/sync` reaches peers.
    pub fn with_broadcaster(db: Arc<crate::DB<S>>, broadcaster: Arc<dyn TxnBroadcaster>) -> Self {
        Self::build(db, Some(broadcaster))
    }

    fn build(db: Arc<crate::DB<S>>, broadcaster: Option<Arc<dyn TxnBroadcaster>>) -> Self {
        let blockstore = Arc::new(DefraBlockstore::new(db.store().clone(), true));
        let merge_handler = Arc::new(DbMergeHandler::new(db.clone(), blockstore.clone()));
        if let Some(kms) = db.kms() {
            merge_handler.set_kms(kms);
        }
        Self {
            db,
            blockstore,
            merge_handler,
            broadcaster,
        }
    }

    pub fn database(&self) -> &Arc<crate::DB<S>> {
        &self.db
    }

    pub fn merge_handler(&self) -> &Arc<DbMergeHandler<S, DefraBlockstore<S>>> {
        &self.merge_handler
    }

    pub async fn document_refs(&self) -> Result<Vec<BrowserSyncDocumentRef>, BrowserSyncError> {
        let txn = self
            .db
            .new_txn(true)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let datastore = txn
            .datastore()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let systemstore = txn
            .systemstore()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let mut refs = Vec::new();

        for name in self
            .db
            .list_collections()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
        {
            let Some(collection) = self
                .db
                .get_collection(&name)
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
            else {
                continue;
            };
            let prefix = format!("/d/{}/", collection.collection_id()).into_bytes();
            let prefix_len = prefix.len();
            let mut iterator = datastore
                .iterator(IterOptions::new().with_prefix(prefix).with_keys_only(true))
                .await
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;

            while let Some(pair) = iterator
                .next()
                .await
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
            {
                let Ok(doc_short_id) =
                    storage::keys::doc_id_index::decode_doc_short_id(&pair.key[prefix_len..])
                else {
                    continue;
                };
                let Some(doc_id) = crate::docid::map::get_doc_id(&systemstore, doc_short_id)
                    .await
                    .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
                else {
                    continue;
                };
                refs.push(BrowserSyncDocumentRef {
                    doc_id,
                    collection_id: collection.collection_id().to_string(),
                    doc_short_id,
                });
            }
            iterator
                .close()
                .await
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        }

        refs.sort_by(|left, right| left.doc_id.cmp(&right.doc_id));
        refs.dedup_by(|left, right| left.doc_id == right.doc_id);
        Ok(refs)
    }

    pub async fn document_ref(
        &self,
        doc_id: &str,
    ) -> Result<Option<BrowserSyncDocumentRef>, BrowserSyncError> {
        let txn = self
            .db
            .new_txn(true)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let systemstore = txn
            .systemstore()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let Some(doc_ref) = crate::docid::map::get_doc_ref(&systemstore, doc_id)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
        else {
            return Ok(None);
        };
        let canonical_doc_id = crate::docid::map::get_doc_id(&systemstore, doc_ref.doc_short_id)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
            .ok_or_else(|| {
                BrowserSyncError::Storage(format!("document {doc_id} has no canonical ID mapping"))
            })?;

        for name in self
            .db
            .list_collections()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
        {
            let Some(collection) = self
                .db
                .get_collection(&name)
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
            else {
                continue;
            };
            if collection.resolved_root_id() == doc_ref.collection_short_id {
                return Ok(Some(BrowserSyncDocumentRef {
                    doc_id: canonical_doc_id,
                    collection_id: collection.collection_id().to_string(),
                    doc_short_id: doc_ref.doc_short_id,
                }));
            }
        }

        Ok(None)
    }

    /// The document's current composite heads, without loading its DAG.
    ///
    /// This is what a push would offer, and it is read on its own so that
    /// deciding not to push costs a headstore lookup rather than the whole
    /// history.
    pub async fn document_roots(
        &self,
        document_ref: &BrowserSyncDocumentRef,
    ) -> Result<Vec<String>, BrowserSyncError> {
        let txn = self
            .db
            .new_txn(true)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        head_roots(&txn, document_ref).await
    }

    /// Whether `peer` still needs what this node holds for the document.
    ///
    /// A push can only ever deliver a block the peer has not got. When the
    /// peer's last recorded heads are this node's current heads, there is no
    /// such block, and offering the document anyway is somebody else's
    /// document coming back unchanged — which the receiver has every right to
    /// refuse, and which used to end the exchange it rode in on.
    ///
    /// Absent a record the answer is yes: a node that has never exchanged this
    /// document with the peer must assume the peer lacks it.
    ///
    /// One transaction, because a full sync asks this of every document it
    /// holds before it loads any of them.
    pub async fn peer_needs_document(
        &self,
        peer: &str,
        document_ref: &BrowserSyncDocumentRef,
    ) -> Result<bool, BrowserSyncError> {
        let txn = self
            .db
            .new_txn(true)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let recorded = read_peer_roots(
            &txn.peerstore()
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?,
            peer,
            &document_ref.doc_id,
        )
        .await?;
        let Some(recorded) = recorded else {
            return Ok(true);
        };
        Ok(recorded != head_roots(&txn, document_ref).await?)
    }

    /// The heads `peer` was last known to hold for a document.
    pub async fn peer_roots(
        &self,
        peer: &str,
        doc_id: &str,
    ) -> Result<Option<Vec<String>>, BrowserSyncError> {
        let txn = self
            .db
            .new_txn(true)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        read_peer_roots(
            &txn.peerstore()
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?,
            peer,
            doc_id,
        )
        .await
    }

    /// Record the heads `peer` holds: the roots it handed over, or the roots
    /// it accepted.
    ///
    /// Durable, because the question it answers outlives a session. A second
    /// `sync()`, a reconnect and a reopened store all start by asking what the
    /// peer needs, and a record that died with the session would answer
    /// "everything" every time.
    ///
    /// A whole exchange in one transaction: a pull page carries up to
    /// sixty-four documents and an initial pull is every document there is.
    pub async fn record_peer_roots(
        &self,
        peer: &str,
        documents: &[(String, Vec<String>)],
    ) -> Result<(), BrowserSyncError> {
        if documents.is_empty() {
            return Ok(());
        }
        let txn = self
            .db
            .new_txn(false)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        {
            // The view borrows the transaction, and a transaction with a
            // reference outstanding will not commit.
            let peerstore = txn
                .peerstore()
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
            for (doc_id, roots) in documents {
                peerstore
                    .set(
                        &BrowserSyncHeadKey::new(peer, doc_id).bytes(),
                        sorted_roots(roots.iter().cloned()).join("\n").as_bytes(),
                    )
                    .await
                    .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
            }
        }
        txn.commit()
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))
    }

    /// Whether every block in a validated payload is already held here, in
    /// which case merging it cannot change anything.
    ///
    /// The blocks are content-addressed, so holding the CID is holding the
    /// block: a payload this answers `true` for has nothing in it this node
    /// has not already seen.
    pub async fn holds_every_block(
        &self,
        document: &ValidatedBrowserSyncDocument,
    ) -> Result<bool, BrowserSyncError> {
        for (cid, _) in &document.blocks {
            if !self
                .blockstore
                .has(cid)
                .await
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub async fn load_document(
        &self,
        document_ref: &BrowserSyncDocumentRef,
    ) -> Result<Option<BrowserSyncDocument>, BrowserSyncError> {
        let txn = self
            .db
            .new_txn(true)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let headstore = txn
            .headstore()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let blockstore = txn
            .blockstore()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let encstore = txn
            .encstore()
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
        let roots =
            load_latest_composite_head_cids(&headstore, &blockstore, document_ref.doc_short_id)
                .await;
        if roots.is_empty() {
            return Ok(None);
        }

        let mut seen = HashSet::new();
        let mut blocks = Vec::new();
        let mut total_bytes = 0usize;
        for root in &roots {
            let Some(root_data) = blockstore
                .get(&root.to_bytes())
                .await
                .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
            else {
                return Err(BrowserSyncError::Storage(format!(
                    "document root {root} is missing"
                )));
            };
            for (cid, data) in load_push_dag_blocks(&blockstore, &encstore, *root, root_data).await
            {
                if !seen.insert(cid) {
                    continue;
                }
                total_bytes = total_bytes.saturating_add(data.len());
                if total_bytes > MAX_SYNC_PAYLOAD_BYTES {
                    return Err(BrowserSyncError::TooLarge(format!(
                        "document {} exceeds {} bytes",
                        document_ref.doc_id, MAX_SYNC_PAYLOAD_BYTES
                    )));
                }
                blocks.push(BrowserSyncBlock {
                    cid: cid.to_string(),
                    data: hex::encode(data),
                });
            }
        }

        let document = BrowserSyncDocument {
            doc_id: document_ref.doc_id.clone(),
            collection_id: document_ref.collection_id.clone(),
            roots: roots.into_iter().map(|cid| cid.to_string()).collect(),
            blocks,
            // Grants are node-local state, not part of the DAG a pull carries.
            relationships: Vec::new(),
        };
        self.validate_document(&document)?;
        Ok(Some(document))
    }

    pub async fn apply_document(
        &self,
        document: &BrowserSyncDocument,
        creator: &str,
    ) -> Result<(), BrowserSyncError> {
        let document = self.validate_document(document)?;
        self.apply_validated_document(document, creator).await
    }

    pub async fn apply_validated_document(
        &self,
        document: ValidatedBrowserSyncDocument,
        creator: &str,
    ) -> Result<(), BrowserSyncError> {
        let block_refs: Vec<_> = document
            .blocks
            .iter()
            .map(|(cid, data)| (cid, data.as_ref()))
            .collect();
        self.blockstore
            .put_many(&block_refs)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;

        for root in &document.roots {
            let data = document
                .blocks
                .iter()
                .find_map(|(cid, data)| (cid == root).then_some(data.as_ref()))
                .expect("validated roots are present in blocks");
            match self
                .merge_handler
                .handle_block(
                    root,
                    data,
                    BlockMetadata::normal(
                        &document.doc_id,
                        &document.collection_id,
                        creator,
                        None,
                        false,
                    ),
                )
                .await
                .map_err(|error| BrowserSyncError::Merge(error.to_string()))?
            {
                MergeOutcome::Merged | MergeOutcome::Skipped { terminal: true, .. } => {}
                MergeOutcome::Skipped { reason, .. } | MergeOutcome::Rejected { reason } => {
                    return Err(BrowserSyncError::Merge(reason));
                }
                _ => return Err(BrowserSyncError::Merge("unsupported merge outcome".into())),
            }
        }

        let cids: Vec<_> = document.blocks.iter().map(|(cid, _)| *cid).collect();
        self.blockstore
            .mark_batch_as_merged(&cids)
            .await
            .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;

        self.announce_merged_document(&document).await;
        Ok(())
    }

    /// Hand a merged fragment to the P2P stack, the way a committed local
    /// write is handed over by `SyncTxnBroadcaster`.
    ///
    /// `/sync` is an ingress from outside the network, as a GraphQL mutation
    /// is: this node is where the document entered, so no peer has it and no
    /// peer will announce it. That is why this is unconditional rather than
    /// gated on `rebroadcast_on_merge`, which governs *re*-announcing blocks
    /// that already reached this node through the network.
    ///
    /// The creator is the DID verified from the genesis signature, never the
    /// caller who delivered the push, so a receiving peer registers the owner
    /// this node proved rather than the one its sender claims.
    async fn announce_merged_document(&self, document: &ValidatedBrowserSyncDocument) {
        let Some(broadcaster) = self.broadcaster.as_ref() else {
            return;
        };

        // Only used for logging on the broadcast path; an unknown collection
        // is not a reason to withhold the announcement.
        let collection_name = self
            .db
            .find_collection_by_id(&document.collection_id)
            .ok()
            .flatten()
            .map(|collection| collection.name().to_string())
            .unwrap_or_default();

        // A replicator with a filter on this collection is skipped unless the
        // push carries a document to match against, and a fragment is blocks
        // rather than a document, so the merged state is read back.
        let document_json = self
            .merged_document_json(&document.doc_id, &document.collection_id)
            .await;

        for root in &document.roots {
            let Some(block) = document
                .blocks
                .iter()
                .find_map(|(cid, data)| (cid == root).then(|| data.clone()))
            else {
                continue;
            };

            broadcaster
                .broadcast_update(TxnBroadcastEvent {
                    collection_name: collection_name.clone(),
                    collection_id: document.collection_id.clone(),
                    doc_id: document.doc_id.clone(),
                    doc_cid: *root,
                    doc_block: block,
                    document_json: document_json.clone(),
                    collection_block: None,
                    creator_did: document.verified_genesis_creator.clone(),
                })
                .await;
        }
    }

    /// The merged document as JSON, for replicator filters to match against.
    ///
    /// A pushed fragment carries blocks rather than a document body, so this
    /// reads back what the merge produced. Failing to read it is not a reason
    /// to withhold the announcement: unfiltered replicators and gossip do not
    /// need it.
    async fn merged_document_json(
        &self,
        doc_id: &str,
        collection_id: &str,
    ) -> Option<serde_json::Value> {
        let txn = self.db.new_txn(true).await.ok()?;
        let doc_ref = crate::docid::map::get_doc_ref(&txn.systemstore().ok()?, doc_id)
            .await
            .ok()
            .flatten()?;

        let mut key = format!("/d/{collection_id}/").into_bytes();
        key.extend_from_slice(&storage::keys::doc_id_index::encode_doc_short_id(
            doc_ref.doc_short_id,
        ));
        let encoded = txn.datastore().ok()?.get(&key).await.ok().flatten()?;

        let document = document::Document::from_cbor(&encoded).ok()?;
        Some(serde_json::Value::Object(
            document.to_map().ok()?.into_iter().collect(),
        ))
    }
}

/// A document's composite heads, in the canonical order.
async fn head_roots<S: Store + 'static>(
    txn: &crate::txn::DbTxn<S>,
    document_ref: &BrowserSyncDocumentRef,
) -> Result<Vec<String>, BrowserSyncError> {
    let headstore = txn
        .headstore()
        .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
    let blockstore = txn
        .blockstore()
        .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
    Ok(sorted_roots(
        load_latest_composite_head_cids(&headstore, &blockstore, document_ref.doc_short_id)
            .await
            .into_iter()
            .map(|cid| cid.to_string()),
    ))
}

/// What a peer was last recorded as holding, or `None` where nothing was.
async fn read_peer_roots(
    peerstore: &datastore::NamespaceView,
    peer: &str,
    doc_id: &str,
) -> Result<Option<Vec<String>>, BrowserSyncError> {
    let Some(stored) = peerstore
        .get(&BrowserSyncHeadKey::new(peer, doc_id).bytes())
        .await
        .map_err(|error| BrowserSyncError::Storage(error.to_string()))?
    else {
        return Ok(None);
    };
    let stored = std::str::from_utf8(&stored)
        .map_err(|error| BrowserSyncError::Storage(error.to_string()))?;
    // A document with no heads is not one a push can carry, so the empty
    // record is the empty set rather than a set holding an empty root.
    Ok(Some(if stored.is_empty() {
        Vec::new()
    } else {
        stored.split('\n').map(ToString::to_string).collect()
    }))
}

/// Root CIDs in a canonical order, so that two head sets compare equal exactly
/// when they hold the same roots.
fn sorted_roots(roots: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut roots: Vec<String> = roots.into_iter().collect();
    roots.sort();
    roots.dedup();
    roots
}
