//! A collection block names documents to merge, and reaches the head store
//! without passing the validator that governs the collection it names.

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

/// A collection block linking `documents`, as a peer holds it before sending.
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
    let mut block = Block::new(
        CrdtDelta::Collection(CollectionDeltaPayload {
            schema_version_id: collection_id.to_string(),
            priority: 1,
        }),
        vec![],
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
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(
            DB::open_from_arc_with_options(store.clone(), DbOptions::default())
                .await
                .unwrap(),
        );
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

#[tokio::test]
async fn a_collection_block_for_a_governed_collection_installs_no_head() {
    let node = Node::with_branchable_grants(Arc::new(RejectEverything)).await;
    let peer = signer();
    // The linked composite is not held, so no composite is judged at all: the
    // head would otherwise install on the strength of the block alone.
    let grant = genesis("col-grants", "writer", &peer.did, &peer);
    let block = collection_block("col-grants", &[&grant], &peer);

    let outcome = block.merge(&node, &peer.did).await.unwrap();

    assert!(
        matches!(&outcome, MergeOutcome::Rejected { reason } if reason.contains("Grants")),
        "{outcome:?}"
    );
    assert!(node.collection_heads("Grants").await.is_empty());
    assert!(node.doc_ids("Grants").await.is_empty());
}

#[tokio::test]
async fn a_governed_collection_block_whose_composites_are_held_installs_no_head() {
    let node = Node::with_branchable_grants(Arc::new(RejectEverything)).await;
    let peer = signer();
    let grant = genesis("col-grants", "writer", &peer.did, &peer);
    grant.store(&node).await;
    let block = collection_block("col-grants", &[&grant], &peer);

    let outcome = block.merge(&node, &peer.did).await.unwrap();

    assert!(
        matches!(&outcome, MergeOutcome::Rejected { reason } if reason.contains("Grants")),
        "{outcome:?}"
    );
    assert!(node.collection_heads("Grants").await.is_empty());
    assert!(node.doc_ids("Grants").await.is_empty());
}

#[tokio::test]
async fn a_collection_block_for_an_ungoverned_collection_still_merges() {
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

#[tokio::test]
async fn a_local_write_appends_no_collection_block_to_a_governed_collection() {
    let node = Node::with_branchable_grants(Arc::new(RejectEverything)).await;
    let writer = signer();
    let document = format!(r#"{{"writer": "{}"}}"#, writer.did);

    node.create_locally("Grants", &document).await;
    node.create_locally("Ledgers", &document).await;

    assert!(node.collection_heads("Grants").await.is_empty());
    // The ungoverned collection beside it is branchable exactly as before.
    assert_eq!(node.collection_heads("Ledgers").await.len(), 1);
}
