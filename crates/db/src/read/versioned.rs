//! Versioned document fetcher for CID-based time-travel queries.
//!
//! This module reconstructs documents at specific historical versions by:
//! 1. Walking the merkle DAG backwards from target CID to genesis
//! 2. Collecting all blocks in the path
//! 3. Replaying CRDT deltas forward to reconstruct document state

use cid::Cid;
use crdt::stored::StoredField;
use crdt::{CausalBlock, TaggedBlock};
use defra_core::block::{Block, CrdtDelta, Encryption, FieldDelta};
use document::Document;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use schema::{CType, CollectionVersion, FieldDescription, FieldKind, ScalarKind};
use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::Arc;
use storage::corekv::Store;

use crate::error::{Error, Result};
use crate::txn::{DbTxn, LentTxn};

const BLOCK_NOT_FOUND: &str =
    "seek failed: (version fetcher) failed to get block in blockstore: ipld: could not find";

/// Fetcher for reconstructing documents at specific historical versions.
///
/// Given a CID, this fetcher walks backwards through the merkle DAG,
/// collects all blocks from the target CID to genesis, then replays
/// the CRDT deltas in forward order to reconstruct the document state.
pub struct VersionedFetcher<S: Store> {
    /// The transaction it reads, when it has one; a fetcher without one only
    /// decrypts through a transaction it is given.
    txn: Option<LentTxn<S>>,
    kms: Option<Arc<dyn kms::KmsService>>,
    caller_identity: Option<identity::Did>,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
}

impl<S: Store> VersionedFetcher<S> {
    /// Create a new versioned fetcher with a shared transaction
    pub fn new(txn: LentTxn<S>) -> Self {
        Self {
            txn: Some(txn),
            kms: None,
            caller_identity: None,
            prepared_inputs: None,
        }
    }

    /// Create a versioned fetcher that resolves encrypted deltas through the KMS.
    pub fn with_kms(
        txn: Option<LentTxn<S>>,
        kms: Option<Arc<dyn kms::KmsService>>,
        caller_identity: Option<identity::Did>,
    ) -> Self {
        Self {
            txn,
            kms,
            caller_identity,
            prepared_inputs: None,
        }
    }

    pub(crate) fn with_prepared_inputs(
        mut self,
        inputs: Option<query::prepared::PreparedInputMemo>,
    ) -> Self {
        self.prepared_inputs = inputs;
        self
    }

    /// Reconstruct a document at the specified CID.
    ///
    /// Returns the document state as it existed when the commit at `cid` was created.
    /// If `expected_doc_id` is provided, validates that the CID belongs to that document.
    pub async fn get_document_at_cid(
        &self,
        cid_str: &str,
        expected_doc_id: Option<&str>,
        collection: Arc<CollectionVersion>,
    ) -> Result<Document> {
        let docs = self
            .get_documents_at_cid(cid_str, expected_doc_id, None, collection)
            .await?;
        docs.into_iter().next().ok_or_else(|| {
            Error::Serialization("cid either does not exist or belong to document".to_string())
        })
    }

    /// Reconstruct documents at the specified CID.
    ///
    /// For document-level CIDs, returns a single document.
    /// For collection-level CIDs (branchable collections), walks the collection DAG
    /// and returns all documents visible at that collection state.
    ///
    /// `collection_short_id` gates membership: documents of another collection
    /// are dropped before ACP and reconstruction.
    pub async fn get_documents_at_cid(
        &self,
        cid_str: &str,
        expected_doc_id: Option<&str>,
        collection_short_id: Option<u32>,
        collection: Arc<CollectionVersion>,
    ) -> Result<Vec<Document>> {
        let txn = self.txn.as_deref().ok_or(Error::TxnNotActive)?;

        // Parse the CID
        let target_cid = Self::parse_cid(cid_str)?;

        // Load the target block first to validate and get doc info
        let target_block = self.load_block(txn, &target_cid).await?;

        // Check if this is a collection block (branchable collection CID)
        if matches!(&target_block.delta, CrdtDelta::Collection(_)) {
            let expected = match expected_doc_id {
                Some(expected) => Some(self.canonical_doc_id(txn, expected).await?),
                None => None,
            };
            return self
                .get_documents_at_collection_cid(
                    txn,
                    &target_cid,
                    &target_block,
                    collection_short_id,
                    expected,
                    collection,
                )
                .await;
        }

        // Regular document CID path
        let owners = self
            .resolve_doc_ids(txn, &target_cid, &target_block)
            .await?
            .ok_or_else(|| {
                Error::Serialization("cid either does not exist or belong to document".to_string())
            })?;
        let selected_owners = if let Some(expected) = expected_doc_id {
            let canonical_expected = self.canonical_doc_id(txn, expected).await?;
            if !owners.iter().any(|owner| owner == &canonical_expected) {
                return Err(Error::Serialization(
                    "cid either does not exist or belong to document".to_string(),
                ));
            }
            vec![canonical_expected]
        } else if owners.is_empty() {
            return Err(Error::Serialization(
                "cid either does not exist or belong to document".to_string(),
            ));
        } else {
            owners
        };

        // Membership gate, before ACP and the DAG walk.
        let mut members = Vec::with_capacity(selected_owners.len());
        for owner in selected_owners {
            if self.is_member(txn, collection_short_id, &owner).await? {
                members.push(owner);
            }
        }
        if members.is_empty() {
            return Ok(Vec::new());
        }

        // Collect all blocks from target CID back to genesis
        let blocks = self
            .collect_blocks_to_genesis(txn, &target_cid, &target_block)
            .await?;

        // Sort blocks by priority (ascending) for forward replay
        let mut sorted_blocks: Vec<(Cid, Block)> = blocks.into_iter().collect();
        sorted_blocks.sort_by_key(|(_, block)| block.delta.priority());

        let mut documents = Vec::with_capacity(members.len());
        for doc_id in members {
            documents.push(replay_deltas(&collection, &sorted_blocks, &doc_id)?);
        }
        Ok(documents)
    }

    /// Whether `doc_id` belongs to `collection_short_id` (`None` skips the gate).
    async fn is_member(
        &self,
        txn: &DbTxn<S>,
        collection_short_id: Option<u32>,
        doc_id: &str,
    ) -> Result<bool> {
        let Some(col_short_id) = collection_short_id else {
            return Ok(true);
        };
        let systemstore = txn.systemstore()?;
        Ok(
            crate::docid::map::get_doc_short_id(&systemstore, col_short_id, doc_id)
                .await?
                .is_some(),
        )
    }

    /// Reconstruct documents from a collection-level CID.
    ///
    /// Walks the collection DAG backwards to find all document composite CIDs,
    /// then reconstructs each unique document at its latest version up to
    /// the target collection state.
    async fn get_documents_at_collection_cid(
        &self,
        txn: &DbTxn<S>,
        start_cid: &Cid,
        start_block: &Block,
        collection_short_id: Option<u32>,
        expected_doc_id: Option<String>,
        collection: Arc<CollectionVersion>,
    ) -> Result<Vec<Document>> {
        // Walk the collection DAG backwards to find all document composite CIDs.
        // Each collection block links to one document composite block.
        // We track doc_id → (priority, composite_cid) keeping highest priority per doc.
        let mut doc_composites: RapidHashMap<String, (u64, Cid)> = RapidHashMap::new();
        let mut visited: RapidHashSet<Cid> = RapidHashSet::new();
        let mut queue: VecDeque<(Cid, Block)> = VecDeque::new();

        visited.insert(*start_cid);
        queue.push_back((*start_cid, start_block.clone()));

        while let Some((_col_cid, col_block)) = queue.pop_front() {
            // Extract document composite CID from collection block's links
            if let Some(ref links) = col_block.links {
                for link in links {
                    let doc_composite_cid = link.link;
                    // Load the document composite block to get its doc_id and priority
                    if let Ok(doc_block) = self.load_block(txn, &doc_composite_cid).await {
                        if let Some(doc_id) = self
                            .resolve_doc_ids(txn, &doc_composite_cid, &doc_block)
                            .await?
                            .and_then(|owners| owners.into_iter().next())
                        {
                            // Membership and docID gates; a mismatch is a skip.
                            if !self.is_member(txn, collection_short_id, &doc_id).await?
                                || expected_doc_id
                                    .as_deref()
                                    .is_some_and(|expected| expected != doc_id)
                            {
                                continue;
                            }
                            let priority = doc_block.delta.priority();
                            match doc_composites.get(&doc_id) {
                                None => {
                                    doc_composites.insert(doc_id, (priority, doc_composite_cid));
                                }
                                Some((existing_priority, _)) => {
                                    if priority > *existing_priority {
                                        doc_composites
                                            .insert(doc_id, (priority, doc_composite_cid));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Traverse collection heads (previous collection blocks)
            if let Some(ref heads) = col_block.heads {
                for head_cid in heads {
                    if !visited.contains(head_cid) {
                        visited.insert(*head_cid);
                        if let Ok(head_block) = self.load_block(txn, head_cid).await {
                            queue.push_back((*head_cid, head_block));
                        }
                    }
                }
            }
        }

        // Reconstruct each document at its specific composite CID
        let mut documents = Vec::new();
        for (_priority, composite_cid) in doc_composites.values() {
            let composite_block = match self.load_block(txn, composite_cid).await {
                Ok(b) => b,
                Err(_) => continue,
            };
            let doc_id = match self
                .resolve_doc_ids(txn, composite_cid, &composite_block)
                .await?
                .and_then(|owners| owners.into_iter().next())
            {
                Some(id) => id,
                None => continue,
            };

            // Collect all blocks from this composite back to genesis
            let blocks = self
                .collect_blocks_to_genesis(txn, composite_cid, &composite_block)
                .await?;

            let mut sorted_blocks: Vec<(Cid, Block)> = blocks.into_iter().collect();
            sorted_blocks.sort_by_key(|(_, block)| block.delta.priority());

            let document = replay_deltas(&collection, &sorted_blocks, &doc_id)?;
            documents.push(document);
        }

        Ok(documents)
    }

    /// Parse a CID string, returning appropriate errors for invalid/unknown CIDs.
    fn parse_cid(cid_str: &str) -> Result<Cid> {
        Cid::from_str(cid_str).map_err(|_e| {
            // Go's CID library is more lenient: a CIDv1-shaped string decodes
            // and then misses in the blockstore, so surface the blockstore
            // miss (an error the runner propagates) rather than the
            // does-not-exist string it swallows as an empty result.
            if Self::looks_like_cidv1(cid_str) {
                Error::Serialization(BLOCK_NOT_FOUND.to_string())
            } else {
                Error::Serialization("invalid cid: selected encoding not supported".to_string())
            }
        })
    }

    /// Check if a string looks like a CIDv1.
    pub fn looks_like_cidv1(s: &str) -> bool {
        if s.len() < 40 {
            return false;
        }
        s.starts_with("bafy")
            || s.starts_with("bafk")
            || s.starts_with("bafz")
            || s.starts_with("bafr")
            || s.starts_with("Qm")
    }

    /// Resolve the DocID owning a block via the systemstore block-ownership
    /// index (`/d/b`). Deltas no longer carry docIDs (Go #4838).
    async fn resolve_doc_ids(
        &self,
        txn: &DbTxn<S>,
        cid: &Cid,
        block: &Block,
    ) -> Result<Option<Vec<String>>> {
        let systemstore = txn.systemstore()?;
        crate::docid::map::resolve_block_doc_ids(&systemstore, cid, block).await
    }

    async fn canonical_doc_id(&self, txn: &DbTxn<S>, doc_id: &str) -> Result<String> {
        let systemstore = txn.systemstore()?;
        let Some(doc_ref) = crate::docid::map::get_doc_ref(&systemstore, doc_id).await? else {
            return Ok(doc_id.to_string());
        };
        crate::docid::map::get_doc_id(&systemstore, doc_ref.doc_short_id)
            .await?
            .ok_or_else(|| {
                Error::InvalidDocument(format!(
                    "document short ID {} has no canonical DocID",
                    doc_ref.doc_short_id
                ))
            })
    }

    /// Collect all blocks from target CID back to genesis using BFS.
    ///
    /// Returns a map of CID -> Block for all blocks in the history.
    async fn collect_blocks_to_genesis(
        &self,
        txn: &DbTxn<S>,
        start_cid: &Cid,
        start_block: &Block,
    ) -> Result<RapidHashMap<Cid, Block>> {
        let mut blocks = RapidHashMap::new();
        let mut visited: RapidHashSet<Cid> = RapidHashSet::new();
        let mut queue: VecDeque<Cid> = VecDeque::new();

        // Start with the target block
        blocks.insert(*start_cid, start_block.clone());
        visited.insert(*start_cid);

        // Queue up heads for traversal
        if let Some(ref heads) = start_block.heads {
            for head_cid in heads {
                queue.push_back(*head_cid);
            }
        }

        // Also traverse links (for composite blocks that link to field blocks)
        if let Some(ref links) = start_block.links {
            for link in links {
                if !visited.contains(&link.link) {
                    queue.push_back(link.link);
                }
            }
        }

        // BFS traversal
        while let Some(cid) = queue.pop_front() {
            if visited.contains(&cid) {
                continue;
            }
            visited.insert(cid);

            let block = match self.load_block(txn, &cid).await {
                Ok(b) => b,
                Err(_) => continue, // Skip missing blocks (genesis reached)
            };

            // Queue heads for further traversal
            if let Some(ref heads) = block.heads {
                for head_cid in heads {
                    if !visited.contains(head_cid) {
                        queue.push_back(*head_cid);
                    }
                }
            }

            // Queue links for composite blocks
            if let Some(ref links) = block.links {
                for link in links {
                    if !visited.contains(&link.link) {
                        queue.push_back(link.link);
                    }
                }
            }

            blocks.insert(cid, block);
        }

        self.decrypt_blocks(txn, start_cid, &mut blocks).await?;
        Ok(blocks)
    }

    /// Load a block from blockstore by CID, its payload as stored.
    async fn load_block(&self, txn: &DbTxn<S>, cid: &Cid) -> Result<Block> {
        let blockstore = txn.blockstore()?;

        let key = cid.to_bytes();
        let data = blockstore
            .get(&key)
            .await
            .map_err(Error::Storage)?
            .ok_or_else(|| Error::Serialization(BLOCK_NOT_FOUND.to_string()))?;

        Block::from_dag_cbor(&data)
            .map_err(|e| Error::Serialization(format!("Failed to decode block: {}", e)))
    }

    /// Decrypts the encrypted field deltas of a walk, with every distinct
    /// key it needs asked for in one request. A block whose key is not
    /// released is left out of the history, as a block that cannot be read
    /// is; the walk's own `start` block fails the read instead.
    async fn decrypt_blocks(
        &self,
        txn: &DbTxn<S>,
        start: &Cid,
        blocks: &mut RapidHashMap<Cid, Block>,
    ) -> Result<()> {
        let mut cids: Vec<Cid> = blocks
            .values()
            .filter(|block| block.delta.field().is_some())
            .filter_map(|block| block.encryption)
            .collect();
        if cids.is_empty() {
            return Ok(());
        }
        cids.sort_unstable();
        cids.dedup();
        let keys = self.read_keys(txn, &cids).await?;
        let mut unreadable = Vec::new();
        for (cid, block) in blocks.iter_mut() {
            let Some(encryption_cid) = block.encryption else {
                continue;
            };
            let Some(payload) = block.delta.field_data_mut() else {
                continue;
            };
            let plaintext = keys
                .get(&encryption_cid)
                .ok_or_else(|| format!("encryption key {encryption_cid} is unavailable"))
                .and_then(|key| key.clone())
                .and_then(|key| decrypt_with_key(payload, &key));
            match plaintext {
                Ok(plaintext) => *payload = plaintext,
                Err(error) if cid == start => return Err(Error::Serialization(error)),
                Err(error) => {
                    tracing::debug!(
                        block = %cid,
                        %encryption_cid,
                        %error,
                        "historical block left out: its key is not released"
                    );
                    unreadable.push(*cid);
                }
            }
        }
        for cid in unreadable {
            blocks.remove(&cid);
        }
        Ok(())
    }

    /// One delta's decryption through the same key path as a walk's.
    #[cfg(test)]
    async fn decrypt_block_data(
        &self,
        txn: &DbTxn<S>,
        data: &[u8],
        encryption_cid: &Cid,
    ) -> Result<Vec<u8>> {
        let key = self
            .read_keys(txn, std::slice::from_ref(encryption_cid))
            .await?
            .remove(encryption_cid)
            .unwrap_or_else(|| Err(format!("encryption key {encryption_cid} is unavailable")))
            .map_err(Error::Serialization)?;
        decrypt_with_key(data, &key).map_err(Error::Serialization)
    }

    /// The keys `cids` names, each with the reason it has none: through the
    /// KMS in one request (an attempt with a prepared-input memo reads the
    /// answer from it, or ends asking for it), or, without a KMS, from the
    /// encryption blocks this node stores.
    async fn read_keys(
        &self,
        txn: &DbTxn<S>,
        cids: &[Cid],
    ) -> Result<RapidHashMap<Cid, std::result::Result<[u8; 32], String>>> {
        let Some(kms) = &self.kms else {
            let mut keys = RapidHashMap::with_capacity(cids.len());
            for cid in cids {
                keys.insert(*cid, self.stored_key(txn, cid).await?);
            }
            return Ok(keys);
        };
        let request_context = self.kms_request_context();
        let Some(inputs) = &self.prepared_inputs else {
            let answers = kms
                .get_keys_each(&request_context, cids)
                .await
                .map_err(|e| {
                    Error::Serialization(format!("failed to fetch encryption key: {e}"))
                })?;
            return Ok(cids
                .iter()
                .copied()
                .zip(answers.into_iter().map(|answer| {
                    answer.map_err(|e| format!("failed to fetch encryption key: {e}"))
                }))
                .collect());
        };
        let request = crate::write::prepare::kms_input_request(
            crate::write::prepare::PreparedKmsOperation::ReadKeys {
                cids: cids.iter().map(ToString::to_string).collect(),
                caller_identity: request_context.user_identity().map(ToString::to_string),
            },
        );
        let response = inputs
            .get(&request)
            .ok_or(Error::Query(query::error::QueryError::NeedsInput(request)))?;
        let answers: Vec<crate::write::prepare::PreparedReadKey> =
            serde_json::from_slice(&response).map_err(|error| {
                Error::Serialization(format!("invalid prepared read keys: {error}"))
            })?;
        if answers.len() != cids.len()
            || answers
                .iter()
                .zip(cids)
                .any(|(answer, cid)| answer.cid != cid.to_string())
        {
            return Err(Error::Serialization(
                "prepared read keys do not match the encryption blocks".into(),
            ));
        }
        Ok(cids
            .iter()
            .copied()
            .zip(answers.into_iter().map(|answer| {
                answer.key.and_then(|key| {
                    key.try_into()
                        .map_err(|_| "prepared read key is not 32 bytes".to_string())
                })
            }))
            .collect())
    }

    /// The key an `Encryption` block this node stores carries: the encstore
    /// first, then the blockstore.
    async fn stored_key(
        &self,
        txn: &DbTxn<S>,
        encryption_cid: &Cid,
    ) -> Result<std::result::Result<[u8; 32], String>> {
        let encryption_key = encryption_cid.to_bytes();
        let encstore = txn.encstore()?;
        let blockstore = txn.blockstore()?;
        let encryption_data = if let Some(data) = encstore
            .get(&encryption_key)
            .await
            .map_err(Error::Storage)?
        {
            data
        } else {
            match blockstore
                .get(&encryption_key)
                .await
                .map_err(Error::Storage)?
            {
                Some(data) => data,
                None => {
                    return Ok(Err(format!(
                        "encryption block {} is unavailable",
                        encryption_cid
                    )))
                }
            }
        };
        let encryption = Encryption::from_dag_cbor(&encryption_data)
            .map_err(|e| Error::Serialization(format!("failed to decode encryption block: {e}")))?;
        Ok(encryption
            .key
            .try_into()
            .map_err(|_| "invalid AES-256 encryption key".to_string()))
    }

    /// The KMS request a read key is fetched under: the identity reading, or
    /// the one acting in [`defra_core::RequestContext::ambient`] when the
    /// fetcher names none.
    pub fn kms_request_context(&self) -> kms::RequestContext {
        self.caller_identity
            .clone()
            .or_else(|| {
                defra_core::RequestContext::ambient()
                    .identity()
                    .and_then(|did| identity::Did::new(did).ok())
            })
            .map(kms::RequestContext::with_user)
            .unwrap_or_else(kms::RequestContext::anonymous)
    }
}

/// Replay CRDT deltas to reconstruct document state.
///
/// Blocks should be sorted by priority (ascending) before calling this method.
fn replay_deltas(
    collection: &Arc<CollectionVersion>,
    blocks: &[(Cid, Block)],
    doc_id: &str,
) -> Result<Document> {
    let mut fields: RapidHashMap<String, StoredField> = RapidHashMap::new();
    let mut histories: RapidHashMap<String, Vec<CausalBlock>> = RapidHashMap::new();
    let mut tagged: RapidHashMap<String, Vec<TaggedBlock>> = RapidHashMap::new();
    let mut is_deleted = false;
    let mut max_composite_priority: u64 = 0;

    // An Immutable block replays as the LWW field deltas it replaces.
    let deltas = blocks.iter().flat_map(|(cid, block)| {
        let deltas = match &block.delta {
            CrdtDelta::Immutable(payload) => payload
                .lww_deltas()
                .map(|delta| std::borrow::Cow::Owned(CrdtDelta::Lww(delta)))
                .collect(),
            other => vec![std::borrow::Cow::Borrowed(other)],
        };
        deltas.into_iter().map(move |delta| (cid, block, delta))
    });
    for (cid, block, delta) in deltas {
        if let Some(field_delta) = delta.field() {
            if !causal_history(collection, &mut histories, cid, block, &delta, &field_delta)?
                && !tagged_history(collection, &mut tagged, cid, block, &delta, &field_delta)
            {
                replay_field(collection, &mut fields, &delta, &field_delta);
            }
            continue;
        }
        match delta.as_ref() {
            CrdtDelta::Composite(payload) if payload.priority >= max_composite_priority => {
                // Track document status from composite blocks.
                // The highest-priority composite determines the final status.
                max_composite_priority = payload.priority;
                is_deleted = payload.status == 2;
            }
            _ => {
                // Collection and schema definition deltas are not relevant for document reconstruction
            }
        }
    }

    // A causal field is the replay of its whole history.
    for (name, history) in histories {
        let Some(field) = collection.field_by_name(&name) else {
            continue;
        };
        let Some(crdt) = crdt::field_crdt(field.crdt_type) else {
            continue;
        };
        let Some(causal) = crdt.causal() else {
            continue;
        };
        let state = causal
            .replay(field, &history)
            .and_then(|value| crdt.state(field, 0, &value))
            .map_err(|e| Error::Serialization(e.to_string()))?;
        fields.insert(name, state);
    }

    // A tagged field is the replay of its whole history.
    for (name, history) in tagged {
        let Some((field, tagged)) = collection
            .field_by_name(&name)
            .and_then(|field| Some((field, crdt::field_crdt(field.crdt_type)?.tagged()?)))
        else {
            continue;
        };
        let state = tagged
            .replay(field, &history)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        if let Some(state) = state {
            fields.insert(name, state);
        }
    }

    // Build document from collected field values
    let mut document = Document::new(collection.clone());
    for (field_name, state) in fields {
        // A deleted value (a tombstone) leaves the field unset.
        if state.value.is_empty() {
            continue;
        }
        match state.materialize() {
            Some(value) => document.set_unchecked(field_name, value),
            None => tracing::warn!(
                field_name = %field_name,
                "Undecodable field state during replay"
            ),
        }
    }

    // Set document ID
    if let Ok(doc_id_obj) = document::DocID::from_string(doc_id) {
        document.set_id(doc_id_obj);
    }

    // Set deleted status from composite block
    if is_deleted {
        document.set_deleted(true);
    }

    Ok(document)
}

/// Add a field block to its field's history when the field is causal, and
/// report whether it was. A block the field's type cannot read fails the
/// read: every later block counts in its version, so the history cannot
/// replay without it.
fn causal_history(
    collection: &CollectionVersion,
    histories: &mut RapidHashMap<String, Vec<CausalBlock>>,
    cid: &Cid,
    block: &Block,
    delta: &CrdtDelta,
    field_delta: &FieldDelta<'_>,
) -> Result<bool> {
    let name = field_delta.field_name;
    let Some(field) = collection.field_by_name(name).filter(|field| {
        crdt::field_crdt(field.crdt_type).is_some_and(|crdt| crdt.causal().is_some())
    }) else {
        return Ok(false);
    };
    let op = crdt::delta_crdt(delta, field.crdt_type)
        .map_err(|error| error.to_string())
        .and_then(|writer| {
            crdt::history_op(writer, field, field_delta.data).map_err(|error| error.to_string())
        })
        .map_err(|error| {
            Error::Serialization(format!(
                "field '{name}' block {cid} cannot be read: {error}"
            ))
        })?;
    histories
        .entry(name.to_owned())
        .or_default()
        .push(CausalBlock {
            id: cid.to_bytes(),
            parents: block.heads.iter().flatten().map(Cid::to_bytes).collect(),
            op,
        });
    Ok(true)
}

/// Add a field block to its field's history when the field is tagged, and
/// report whether it was.
fn tagged_history(
    collection: &CollectionVersion,
    histories: &mut RapidHashMap<String, Vec<TaggedBlock>>,
    cid: &Cid,
    block: &Block,
    delta: &CrdtDelta,
    field_delta: &FieldDelta<'_>,
) -> bool {
    let name = field_delta.field_name;
    let Some(field) = collection.field_by_name(name).filter(|field| {
        crdt::field_crdt(field.crdt_type).is_some_and(|crdt| crdt.tagged().is_some())
    }) else {
        return false;
    };
    match crdt::delta_crdt(delta, field.crdt_type) {
        Ok(_) => histories
            .entry(name.to_owned())
            .or_default()
            .push(TaggedBlock {
                cid: cid.to_bytes(),
                parents: block.heads.iter().flatten().map(Cid::to_bytes).collect(),
                priority: field_delta.priority,
                data: field_delta.data.to_vec(),
            }),
        Err(error) => {
            tracing::warn!(field_name = %name, %error, "Refused field block during replay")
        }
    }
    true
}

/// Merge one field block into the replayed states, through the CRDT that
/// wrote it, exactly as a live merge does.
fn replay_field(
    collection: &CollectionVersion,
    fields: &mut RapidHashMap<String, StoredField>,
    delta: &CrdtDelta,
    field_delta: &FieldDelta<'_>,
) {
    let name = field_delta.field_name;
    let declared = collection.field_by_name(name);
    let crdt = match crdt::delta_crdt(delta, declared.map_or(CType::None, |field| field.crdt_type))
    {
        Ok(crdt) => crdt,
        Err(error) => {
            tracing::warn!(field_name = %name, %error, "Refused field block during replay");
            return;
        }
    };
    let undeclared;
    let field = match declared {
        Some(field) => field,
        None => {
            undeclared =
                FieldDescription::new("", name, FieldKind::Scalar(ScalarKind::None), crdt.ctype());
            &undeclared
        }
    };
    let incoming = match crdt.block_state(field, field_delta.priority, field_delta.data) {
        Ok(incoming) => incoming,
        Err(error) => {
            tracing::warn!(field_name = %name, %error, "Undecodable field block during replay");
            return;
        }
    };
    match fields.get_mut(name) {
        Some(state) => {
            if crdt.merge(state, incoming).is_none() {
                tracing::warn!(field_name = %name, "Unmergeable field block during replay");
            }
        }
        None => {
            fields.insert(name.to_owned(), incoming);
        }
    }
}

#[cfg(test)]
#[path = "versioned_retry_tests.rs"]
mod retry_tests;

#[cfg(test)]
#[path = "versioned_replay_tests.rs"]
mod replay_tests;

/// AES-GCM decryption of a historical field delta.
fn decrypt_with_key(data: &[u8], key: &[u8; 32]) -> std::result::Result<Vec<u8>, String> {
    crypto::encryption::aes::decrypt_aes(None, data, key, &[])
        .map_err(|e| format!("failed to decrypt versioned block: {e}"))
}
