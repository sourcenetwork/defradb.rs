//! Updates to a registered protected document, merged through the full ACP
//! merge handler under local ACP.

use acp::DocumentACP;
use acp::LocalDocumentACP;
use acp::MemoryAcpStore;
use blockstore::Blockstore as _;
use blockstore::DefraBlockstore;
use cid::Cid;
use crypto::PrivateKey as _;
use db::database::DB;
use db::merge::acp_merge_handler::AcpMergeHandler;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::Block;
use defra_core::block::CollectionDeltaPayload;
use defra_core::block::CompositeDeltaPayload;
use defra_core::block::CrdtDelta;
use defra_core::block::DAGLink;
use defra_core::block::Signature;
use defra_core::block::SignatureHeader;
use defra_core::block::SignatureType;
use defra_core::merge::BlockMetadata;
use defra_core::merge::MergeBlock;
use defra_core::merge::MergeHandler;
use defra_core::merge::MergeOutcome;
use document::Document;
use document::NormalValue;
use identity::Did;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;
use schema::PolicyDescription;
use std::sync::Arc;
use storage::RegolithStore;

type Blocks = DefraBlockstore<RegolithStore>;

struct Signer {
    key: crypto::Ed25519PrivateKey,
    did: String,
}

fn signer() -> Signer {
    let key = crypto::generate_ed25519().unwrap();
    let did = key.public_key().did().unwrap();
    Signer { key, did }
}

struct Fixture {
    handler: AcpMergeHandler<RegolithStore, Blocks>,
    blockstore: Arc<Blocks>,
    doc_id: String,
    genesis: Cid,
}

impl Fixture {
    /// A replicated document that `owner` then registers in the node's ACP.
    async fn new(owner: &Signer) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(
            CollectionVersion::new(
                "Users",
                "v1",
                "col-users",
                vec![
                    FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                    FieldDescription::new("2", "name", FieldKind::string()),
                ],
            )
            .with_policy(PolicyDescription::new("policy-1", "users")),
        )
        .await
        .unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let acp = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
        let handler = AcpMergeHandler::new(Arc::new(DbMergeHandler::new(db, blockstore.clone())))
            .with_document_acp(acp.clone());

        let mut doc = Document::new();
        doc.set("name", NormalValue::String("Alice".to_string()));
        let genesis = db::block::builder::build_blocks_from_document(&doc, "v1", &blockstore)
            .await
            .unwrap();
        let outcome = handler
            .handle_block(
                &genesis.cid,
                &genesis.block,
                BlockMetadata::normal(&genesis.doc_id, "col-users", &owner.did, None, false),
            )
            .await
            .unwrap();
        assert_eq!(outcome, MergeOutcome::Merged);
        acp.register_doc_object(
            &Did::new(owner.did.clone()).unwrap(),
            "policy-1",
            "users",
            &genesis.doc_id,
        )
        .await
        .unwrap();

        Self {
            handler,
            blockstore,
            doc_id: genesis.doc_id,
            genesis: genesis.cid,
        }
    }

    /// A composite update stored in the blockstore, as a fetched ancestor is.
    async fn update(&self, heads: Vec<Cid>, priority: u64, signer: &Signer) -> (Cid, Vec<u8>) {
        let mut block = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                schema_version_id: "v1".to_string(),
                priority,
                status: 1,
            }),
            heads,
            vec![],
        );
        let value = signer.key.sign(&block.to_dag_cbor().unwrap()).unwrap();
        let signature = Signature::new(
            SignatureHeader::new(
                SignatureType::EdDSA,
                hex::encode(signer.key.public_key().raw()).into_bytes(),
            ),
            value,
        );
        let signature_cid = signature.generate_cid().unwrap();
        self.blockstore
            .put(&signature_cid, &signature.to_dag_cbor().unwrap())
            .await
            .unwrap();
        block.signature = Some(signature_cid);
        let cid = block.generate_cid().unwrap();
        let bytes = block.to_dag_cbor().unwrap();
        self.blockstore.put(&cid, &bytes).await.unwrap();
        (cid, bytes)
    }

    async fn merge(
        &self,
        (cid, bytes): &(Cid, Vec<u8>),
        creator: &str,
        explicit: bool,
    ) -> MergeOutcome {
        self.handler
            .handle_block(
                cid,
                bytes,
                BlockMetadata::normal(&self.doc_id, "col-users", creator, Some("peer"), explicit),
            )
            .await
            .unwrap()
    }

    fn denied(&self, signer: &Signer) -> MergeOutcome {
        MergeOutcome::rejected(format!(
            "signer {} lacks update permission on protected document {}",
            signer.did, self.doc_id
        ))
    }
}

#[tokio::test]
async fn explicit_replay_does_not_bypass_update_permission() {
    let owner = signer();
    let attacker = signer();
    let fixture = Fixture::new(&owner).await;

    let hijack = fixture.update(vec![fixture.genesis], 2, &attacker).await;

    assert_eq!(
        fixture.merge(&hijack, &owner.did, true).await,
        fixture.denied(&attacker)
    );
}

#[tokio::test]
async fn ancestor_is_judged_by_its_own_signer() {
    let owner = signer();
    let attacker = signer();
    let fixture = Fixture::new(&owner).await;

    let (ancestor, _) = fixture.update(vec![fixture.genesis], 2, &attacker).await;
    let child = fixture.update(vec![ancestor], 3, &owner).await;
    assert_eq!(
        fixture.merge(&child, &owner.did, false).await,
        fixture.denied(&attacker)
    );

    let sanctioned = fixture.update(vec![fixture.genesis], 2, &owner).await;
    assert_eq!(
        fixture.merge(&sanctioned, &owner.did, false).await,
        MergeOutcome::Merged
    );
}

#[tokio::test]
async fn composite_linked_from_collection_block_is_judged_by_its_own_signer() {
    let owner = signer();
    let attacker = signer();
    let fixture = Fixture::new(&owner).await;

    let (hijack, _) = fixture.update(vec![fixture.genesis], 2, &attacker).await;
    let payload = CollectionDeltaPayload {
        schema_version_id: "v1".to_string(),
        priority: 1,
    };
    let carrier = Block::new(
        CrdtDelta::Collection(payload.clone()),
        vec![],
        vec![DAGLink::new(&fixture.doc_id, hijack)],
    );
    let carrier_cid = carrier.generate_cid().unwrap();
    let mut metadata = BlockMetadata::normal(
        &fixture.doc_id,
        "col-users",
        &owner.did,
        Some("peer"),
        false,
    );
    metadata.verified_creator = Some(owner.did.clone());

    let outcome = fixture
        .handler
        .inner()
        .process_collection_delta(&carrier_cid, &carrier, &payload, &metadata, 0)
        .await
        .unwrap();

    assert_eq!(outcome, fixture.denied(&attacker));
}

#[tokio::test]
async fn batch_ancestor_is_judged_by_its_own_signer() {
    let owner = signer();
    let attacker = signer();
    let fixture = Fixture::new(&owner).await;

    let (ancestor, _) = fixture.update(vec![fixture.genesis], 2, &attacker).await;
    let (cid, bytes) = fixture.update(vec![ancestor], 3, &owner).await;
    let results = fixture
        .handler
        .handle_block_batch(&[MergeBlock {
            cid,
            block_data: bytes::Bytes::from(bytes),
            doc_id: fixture.doc_id.clone(),
            collection_id: "col-users".to_string(),
            creator: owner.did.clone(),
            sender_peer: Some("peer".to_string()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }])
        .await;

    assert_eq!(results.len(), 1);
    assert_eq!(
        results.into_iter().next().unwrap().unwrap(),
        fixture.denied(&attacker)
    );
}
