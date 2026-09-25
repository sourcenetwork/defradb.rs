//! A collection block of a governed collection is judged by its links: it
//! installs a head once every composite it links and every collection block
//! it supersedes has merged, is rejected when any of them is known rejected,
//! and otherwise waits on what is missing.

use defra_core::block::CollectionDeltaPayload;
use storage::IterOptions;

use super::*;

/// Rejects every composite it is asked about.
struct RejectEverything;

#[async_trait]
impl MergeValidator for RejectEverything {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(MergeVerdict::reject("nothing is accepted here"))
    }
}

/// Accepts every composite it is asked about.
struct AcceptEverything;

#[async_trait]
impl MergeValidator for AcceptEverything {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(MergeVerdict::Accept)
    }
}

/// Rejects what one signer wrote and accepts the rest.
struct RejectSigner(String);

#[async_trait]
impl MergeValidator for RejectSigner {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(match &candidate.signature {
            SignatureStatus::Verified(did) if *did == self.0 => MergeVerdict::reject("forged"),
            _ => MergeVerdict::Accept,
        })
    }
}

/// A collection block linking `documents` after `parents`, as a peer holds
/// it before sending.
struct CollectionBlock {
    cid: Cid,
    bytes: Vec<u8>,
    signature: (Cid, Vec<u8>),
    collection_id: &'static str,
}

fn collection_block(
    collection_id: &'static str,
    documents: &[&Genesis],
    by: &Signer,
) -> CollectionBlock {
    collection_block_after(collection_id, &[], documents, by)
}

fn collection_block_after(
    collection_id: &'static str,
    parents: &[Cid],
    documents: &[&Genesis],
    by: &Signer,
) -> CollectionBlock {
    let mut block = Block::new(
        CrdtDelta::Collection(CollectionDeltaPayload {
            schema_version_id: collection_id.to_string(),
            priority: 1 + parents.len() as u64,
        }),
        parents.to_vec(),
        documents
            .iter()
            .map(|document| DAGLink::new("_head", document.cid))
            .collect(),
    );
    let value = by.key.sign(&block.to_dag_cbor().unwrap()).unwrap();
    let signature = Signature::new(
        SignatureHeader::new(
            SignatureType::EdDSA,
            hex::encode(by.key.public_key().raw()).into_bytes(),
        ),
        value,
    );
    let signature_cid = signature.generate_cid().unwrap();
    block.signature = Some(signature_cid);
    let cid = block.generate_cid().unwrap();
    CollectionBlock {
        cid,
        bytes: block.to_dag_cbor().unwrap(),
        signature: (signature_cid, signature.to_dag_cbor().unwrap()),
        collection_id,
    }
}

impl CollectionBlock {
    /// Hold the block without merging it, as a peer's CAR leaves an ancestor.
    async fn store(&self, node: &Node) {
        let (signature_cid, signature) = &self.signature;
        node.blockstore.put(signature_cid, signature).await.unwrap();
        node.blockstore.put(&self.cid, &self.bytes).await.unwrap();
    }

    async fn merge(
        &self,
        node: &Node,
        creator: &str,
    ) -> Result<MergeOutcome, db::merge::MergeError> {
        let (signature_cid, signature) = &self.signature;
        node.blockstore.put(signature_cid, signature).await.unwrap();
        node.blockstore.put(&self.cid, &self.bytes).await.unwrap();
        node.handler
            .handle_block(
                &self.cid,
                &self.bytes,
                BlockMetadata::normal("", self.collection_id, creator, Some("peer"), false),
            )
            .await
    }
}

impl Node {
    /// A branchable, governed `Grants` beside an ungoverned, branchable
    /// `Ledgers`; both have an `@immutable` `writer`.
    async fn with_branchable_grants(validator: Arc<dyn MergeValidator>) -> Self {
        Self::open_branchable_grants(RegolithStore::in_memory().unwrap(), validator, true).await
    }

    async fn open_branchable_grants(
        store: RegolithStore,
        validator: Arc<dyn MergeValidator>,
        create: bool,
    ) -> Self {
        let store = Arc::new(store);
        let db = Arc::new(
            DB::open_from_arc_with_options(store.clone(), DbOptions::default())
                .await
                .unwrap(),
        );
        if create {
            for (name, id) in [("Grants", "col-grants"), ("Ledgers", "col-ledgers")] {
                let mut writer = FieldDescription::new("2", "writer", FieldKind::string());
                writer.immutable = true;
                db.create_collection(
                    CollectionVersion::new(
                        name,
                        id,
                        id,
                        vec![
                            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                            writer,
                        ],
                    )
                    .as_branchable(),
                )
                .await
                .unwrap();
            }
        }
        db.set_merge_governance(MergeGovernance::new(["Grants"]).with_validator(validator));
        Self::assemble(db, store)
    }

    /// The collection's installed head CIDs.
    async fn collection_heads(&self, collection: &str) -> Vec<Cid> {
        let collection = self.db.get_collection(collection).unwrap().unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let heads = {
            let headstore = txn.headstore().unwrap();
            let prefix = storage::keys::headstore::HeadstoreColKey::collection_prefix(
                collection.resolved_root_id(),
            );
            let prefix_len = prefix.len();
            let mut iter = headstore
                .iterator(IterOptions::new().with_prefix(prefix).with_keys_only(true))
                .await
                .unwrap();
            let mut heads = Vec::new();
            while let Some(pair) = iter.next().await.unwrap() {
                let text = String::from_utf8(pair.key[prefix_len..].to_vec()).unwrap();
                heads.push(text.parse::<Cid>().unwrap());
            }
            iter.close().await.unwrap();
            heads
        };
        let _ = txn.discard();
        heads
    }
}

fn defers_on(outcome: &MergeOutcome, cid: &Cid) -> bool {
    matches!(
        outcome,
        MergeOutcome::Skipped {
            reason,
            terminal: false
        } if reason.contains(&cid.to_string())
    )
}

fn rejects_naming(outcome: &MergeOutcome, cid: &Cid) -> bool {
    matches!(outcome, MergeOutcome::Rejected { reason } if reason.contains(&cid.to_string()))
}

/// The original case: a peer sends the block alone, without the composite it
/// links. No verdict has been taken on that composite, so no head may be
/// installed on the strength of the block; the block waits for it instead.
#[tokio::test]
async fn a_peer_cannot_install_a_head_with_the_block_alone() {
    let node = Node::with_branchable_grants(Arc::new(RejectEverything)).await;
    let peer = signer();
    let grant = genesis("col-grants", "writer", &peer.did, &peer);
    let block = collection_block("col-grants", &[&grant], &peer);

    let outcome = block.merge(&node, &peer.did).await.unwrap();

    assert!(defers_on(&outcome, &grant.cid), "{outcome:?}");
    assert!(node.collection_heads("Grants").await.is_empty());
    assert!(node.doc_ids("Grants").await.is_empty());
    assert_eq!(node.handler.deferred_composites(), 1);

    // The composite arrives and is rejected: nothing releases the block, so
    // the sweep re-judges it and rejects it in turn; the next sweep leaves
    // it alone.
    assert!(matches!(
        grant.merge(&node, &peer.did).await,
        MergeOutcome::Rejected { .. }
    ));
    assert_eq!(node.handler.sweep_unmerged_governed().await, 1);
    assert!(node.collection_heads("Grants").await.is_empty());
    assert_eq!(node.handler.deferred_composites(), 0);
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
    assert!(node.forwarded().is_empty());
}

#[tokio::test]
async fn a_governed_collection_block_waits_for_a_composite_it_does_not_hold() {
    let node = Node::with_branchable_grants(Arc::new(AcceptEverything)).await;
    let peer = signer();
    let grant = genesis("col-grants", "writer", &peer.did, &peer);
    let block = collection_block("col-grants", &[&grant], &peer);

    let outcome = block.merge(&node, &peer.did).await.unwrap();

    assert!(defers_on(&outcome, &grant.cid), "{outcome:?}");
    assert!(node.collection_heads("Grants").await.is_empty());
    assert_eq!(node.handler.deferred_composites(), 1);

    // The composite's own merge releases the block, which installs its head.
    assert_eq!(grant.merge(&node, &peer.did).await, MergeOutcome::Merged);

    assert_eq!(node.collection_heads("Grants").await, vec![block.cid]);
    assert_eq!(node.doc_ids("Grants").await, vec![grant.doc_id.clone()]);
    assert_eq!(node.handler.deferred_composites(), 0);
    // Re-driven under no collection id: marked merged, never pushed as a
    // document write.
    assert_eq!(node.forwarded_with_ids(), vec![(block.cid, String::new())]);
}

#[tokio::test]
async fn a_governed_collection_block_linking_a_rejected_composite_is_rejected() {
    let peer = signer();
    let stranger = signer();
    let node = Node::with_branchable_grants(Arc::new(RejectSigner(stranger.did.clone()))).await;
    let honest = genesis("col-grants", "writer", &peer.did, &peer);
    let forged = genesis("col-grants", "writer", &stranger.did, &stranger);
    honest.store(&node).await;
    forged.store(&node).await;
    let block = collection_block("col-grants", &[&honest, &forged], &peer);

    let outcome = block.merge(&node, &peer.did).await.unwrap();

    assert!(rejects_naming(&outcome, &forged.cid), "{outcome:?}");
    assert!(node.collection_heads("Grants").await.is_empty());
    // The honest composite merged through its own path all the same.
    assert_eq!(node.doc_ids("Grants").await, vec![honest.doc_id.clone()]);
    assert_eq!(node.handler.deferred_composites(), 0);
    // A reject rests on present bytes: the sweep does not re-judge the block.
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
}

#[tokio::test]
async fn a_governed_collection_block_waits_for_its_parent() {
    let node = Node::with_branchable_grants(Arc::new(AcceptEverything)).await;
    let peer = signer();
    let first = genesis("col-grants", "writer", &peer.did, &peer);
    let second = genesis("col-grants", "writer", "someone else", &peer);
    first.store(&node).await;
    second.store(&node).await;
    let parent = collection_block("col-grants", &[&first], &peer);
    let child = collection_block_after("col-grants", &[parent.cid], &[&second], &peer);

    let outcome = child.merge(&node, &peer.did).await.unwrap();

    assert!(defers_on(&outcome, &parent.cid), "{outcome:?}");
    assert!(node.collection_heads("Grants").await.is_empty());
    assert_eq!(node.handler.deferred_composites(), 1);

    // The parent installs and releases the child.
    assert_eq!(
        parent.merge(&node, &peer.did).await.unwrap(),
        MergeOutcome::Merged
    );

    assert!(node.collection_heads("Grants").await.contains(&child.cid));
    assert_eq!(node.handler.deferred_composites(), 0);
    let mut merged = node.doc_ids("Grants").await;
    merged.sort();
    let mut expected = vec![first.doc_id.clone(), second.doc_id.clone()];
    expected.sort();
    assert_eq!(merged, expected);
    assert_eq!(node.forwarded(), vec![child.cid]);
}

#[tokio::test]
async fn a_child_of_a_rejected_collection_block_is_rejected() {
    let peer = signer();
    let stranger = signer();
    let node = Node::with_branchable_grants(Arc::new(RejectSigner(stranger.did.clone()))).await;
    let forged = genesis("col-grants", "writer", &stranger.did, &stranger);
    let honest = genesis("col-grants", "writer", &peer.did, &peer);
    forged.store(&node).await;
    honest.store(&node).await;
    let parent = collection_block("col-grants", &[&forged], &peer);
    parent.store(&node).await;
    let child = collection_block_after("col-grants", &[parent.cid], &[&honest], &peer);

    let outcome = child.merge(&node, &peer.did).await.unwrap();

    // The parent is held, so the walk judges it first and records its
    // rejection; the child supersedes a rejected block.
    assert!(rejects_naming(&outcome, &parent.cid), "{outcome:?}");
    assert!(node.collection_heads("Grants").await.is_empty());
    assert_eq!(node.doc_ids("Grants").await, vec![honest.doc_id.clone()]);
}

/// The index lives in memory, so a restart forgets what the block awaited.
/// The sweep walks the unmerged set and re-judges it.
#[tokio::test]
async fn a_deferred_collection_block_is_rejudged_by_the_sweep_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node");
    let peer = signer();
    let grant = genesis("col-grants", "writer", &peer.did, &peer);
    let block = collection_block("col-grants", &[&grant], &peer);

    {
        let node = Node::open_branchable_grants(
            RegolithStore::open(&path).unwrap(),
            Arc::new(AcceptEverything),
            true,
        )
        .await;
        let outcome = block.merge(&node, &peer.did).await.unwrap();
        assert!(defers_on(&outcome, &grant.cid), "{outcome:?}");
        assert_eq!(node.handler.deferred_composites(), 1);
        node.db.close().await.unwrap();
    }

    let node = Node::open_branchable_grants(
        RegolithStore::open(&path).unwrap(),
        Arc::new(AcceptEverything),
        false,
    )
    .await;
    assert_eq!(node.handler.deferred_composites(), 0);
    // The composite's arrival releases nothing: the index is empty.
    assert_eq!(grant.merge(&node, &peer.did).await, MergeOutcome::Merged);
    assert!(node.collection_heads("Grants").await.is_empty());

    assert_eq!(node.handler.sweep_unmerged_governed().await, 1);

    assert_eq!(node.collection_heads("Grants").await, vec![block.cid]);
    assert_eq!(node.forwarded(), vec![block.cid]);
    // Installed and marked: the next sweep has nothing to re-judge.
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
}

/// Today's behaviour, pinned: an ungoverned collection block drives what it
/// holds, waits on a missing link without being indexed, and installs its
/// head whether or not its parent is held.
#[tokio::test]
async fn an_ungoverned_collection_block_merges_as_before() {
    let node = Node::with_branchable_grants(Arc::new(RejectEverything)).await;
    let peer = signer();
    let ledger = genesis("col-ledgers", "writer", &peer.did, &peer);
    ledger.store(&node).await;
    let block = collection_block("col-ledgers", &[&ledger], &peer);

    assert_eq!(
        block.merge(&node, &peer.did).await.unwrap(),
        MergeOutcome::Merged
    );
    assert_eq!(node.collection_heads("Ledgers").await, vec![block.cid]);
    assert_eq!(node.doc_ids("Ledgers").await, vec![ledger.doc_id.clone()]);

    // A link not held: a retryable skip, no head, and nothing indexed.
    let unheld = genesis("col-ledgers", "writer", "not held", &peer);
    let waiting = collection_block_after("col-ledgers", &[block.cid], &[&unheld], &peer);
    let outcome = waiting.merge(&node, &peer.did).await.unwrap();
    assert!(
        matches!(&outcome, MergeOutcome::Skipped { reason, terminal: false } if reason.contains("not held")),
        "{outcome:?}"
    );
    assert_eq!(node.collection_heads("Ledgers").await, vec![block.cid]);
    assert_eq!(node.handler.deferred_composites(), 0);

    // A parent not held: the head installs regardless.
    let later = genesis("col-ledgers", "writer", "held later", &peer);
    later.store(&node).await;
    let orphan = collection_block_after("col-ledgers", &[waiting.cid], &[&later], &peer);
    assert_eq!(
        orphan.merge(&node, &peer.did).await.unwrap(),
        MergeOutcome::Merged
    );
    assert!(node.collection_heads("Ledgers").await.contains(&orphan.cid));
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
}

#[tokio::test]
async fn a_governed_collections_composites_are_judged_as_before() {
    let node = Node::with_branchable_grants(Arc::new(RejectEverything)).await;
    let peer = signer();
    let grant = genesis("col-grants", "writer", &peer.did, &peer);

    let outcome = grant.merge(&node, &peer.did).await;

    assert!(
        matches!(&outcome, MergeOutcome::Rejected { reason } if reason.contains("nothing is accepted here")),
        "{outcome:?}"
    );
    assert!(node.doc_ids("Grants").await.is_empty());
}

/// A node's own collection block links only composites it has merged, so it
/// passes the judgement by construction: `@branchable` keeps its meaning for
/// a governed collection.
#[tokio::test]
async fn a_local_write_appends_a_collection_block_to_a_governed_collection() {
    let node = Node::with_branchable_grants(Arc::new(RejectEverything)).await;
    let writer = signer();
    let document = format!(r#"{{"writer": "{}"}}"#, writer.did);

    node.create_locally("Grants", &document).await;
    node.create_locally("Ledgers", &document).await;

    assert_eq!(node.collection_heads("Grants").await.len(), 1);
    assert_eq!(node.collection_heads("Ledgers").await.len(), 1);
}
