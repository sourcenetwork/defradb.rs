//! A replicated document whose creator belongs to another deployment, merged
//! by a node that runs its own Vera node and registers such documents for a
//! listed collection. The blocks are signed by a key the node does not hold,
//! as a peer of another deployment would send them.

use std::sync::Arc;

use acp::{DocumentACP as _, DocumentPermission, Identity, ReplicaGrant};
use blockstore::DefraBlockstore;
use cid::Cid;
use crypto::PrivateKey as _;
use db::merge::acp_merge_handler::AcpMergeHandler;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, Signature, SignatureHeader, SignatureType,
};
use defra_core::merge::{BlockMetadata, MergeHandler as _, MergeOutcome};
use defra_node::{EmbeddedVeraActor, ReplicaOwnership, ReplicaRegistration};
use document::{Document, NormalValue};
use identity::Did;
use query::runner::DocFetcher as _;
use zanzibar::types::Subject;

use super::{starting_with, users, POLICY};

/// Nobody is one of this deployment's own actors; the creator gets write.
struct Foreign;

impl ReplicaOwnership for Foreign {
    fn is_own_actor(&self, _collection: &str, _actor: &Did) -> bool {
        false
    }

    fn creator_grants(&self, _collection: &str, creator: &Did, _doc_id: &str) -> Vec<ReplicaGrant> {
        vec![ReplicaGrant::new(
            "writer",
            Subject::Entity(creator.clone()),
        )]
    }
}

/// `block`, signed by `key`, stored with its signature.
async fn signed<B>(key: &crypto::Ed25519PrivateKey, mut block: Block, blocks: &B) -> (Cid, Vec<u8>)
where
    B: blockstore::Blockstore,
{
    block.signature = None;
    let value = key.sign(&block.to_dag_cbor().unwrap()).unwrap();
    let signature = Signature::new(
        SignatureHeader::new(
            SignatureType::EdDSA,
            hex::encode(key.public_key().raw()).into_bytes(),
        ),
        value,
    );
    let signature_cid = signature.generate_cid().unwrap();
    blocks
        .put(&signature_cid, &signature.to_dag_cbor().unwrap())
        .await
        .unwrap();
    block.signature = Some(signature_cid);
    let cid = block.generate_cid().unwrap();
    let bytes = block.to_dag_cbor().unwrap();
    blocks.put(&cid, &bytes).await.unwrap();
    (cid, bytes)
}

fn composite(heads: Vec<Cid>, priority: u64, status: u8) -> Block {
    Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: String::new(),
            priority,
            status,
        }),
        heads,
        vec![],
    )
}

#[test]
fn a_foreign_creators_row_is_registered_by_the_receiving_node_and_its_changes_follow() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = starting_with(
            directory.path(),
            EmbeddedVeraActor::Secp256k1Key(vec![42; 32]),
            Vec::new(),
            None,
            |builder| {
                builder.with_replica_registration(ReplicaRegistration::new(
                    ["Users"],
                    Arc::new(Foreign),
                ))
            },
        )
        .await
        .expect("node");
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let actor = Did::new(vera.self_did().expect("actor DID")).unwrap();
        let policy = vera
            .add_policy(actor.as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");

        let database = node.database().clone();
        let users = database.get_collection("Users").unwrap().unwrap();
        let blocks = Arc::new(DefraBlockstore::new(database.store().clone(), false));
        let handler = AcpMergeHandler::new(Arc::new(DbMergeHandler::new(
            database.clone(),
            blocks.clone(),
        )))
        .with_document_acp(node.document_acp());
        handler.set_strict_replicated_doc_access(true);
        let installed = database.document_acp().unwrap().clone();

        let key = crypto::generate_ed25519().unwrap();
        let creator = Did::new(key.public_key().did().unwrap()).unwrap();
        let mut doc = Document::new(users.schema_arc());
        doc.set("name", NormalValue::String("Alice".into()))
            .unwrap();
        let built = db::block::builder::build_blocks_from_document(
            &storage::Crdts::default(),
            &doc,
            users.schema(),
            &blocks,
        )
        .await
        .unwrap();
        let genesis = signed(&key, Block::from_dag_cbor(&built.block).unwrap(), &*blocks).await;
        let merge = |(cid, bytes): (Cid, Vec<u8>), doc_id: String| {
            let handler = &handler;
            let collection_id = users.collection_id().to_owned();
            async move {
                handler
                    .handle_block(
                        &cid,
                        &bytes,
                        BlockMetadata::normal(
                            &doc_id,
                            &collection_id,
                            "did:key:z6MkPeer",
                            Some("peer"),
                            false,
                        ),
                    )
                    .await
                    .unwrap()
            }
        };

        assert_eq!(
            merge(genesis.clone(), built.doc_id.clone()).await,
            MergeOutcome::Merged
        );
        // The signed genesis names its document by its own content.
        let rows = db::LensedAutoCommitFetcher::new(database.clone())
            .get_all("Users")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        let doc_id = rows[0].id().expect("document ID").to_string();
        installed
            .settle_replica(&policy, "users", &doc_id)
            .await
            .expect("the unit lands on the Vera node");
        // The light client sees the registration once its block is verified.
        // A read the chain answers with its retryable evidence deadline under
        // load is asked again.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while vera
            .get_doc_owner(&policy, "users", &doc_id)
            .await
            .ok()
            .flatten()
            != Some(actor.clone())
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the node did not register the document as its own actor"
            );
            edge::testing::edge()
                .sleep(std::time::Duration::from_millis(200))
                .await;
        }
        let creator_may = |permission| {
            let vera = &vera;
            let (creator, policy, doc_id) = (&creator, &policy, &doc_id);
            async move {
                vera.check_doc_access(
                    &Identity::Authenticated(creator.clone()),
                    permission,
                    policy,
                    "users",
                    doc_id,
                )
                .await
                .unwrap_or(false)
            }
        };
        while !creator_may(DocumentPermission::Update).await {
            assert!(
                std::time::Instant::now() < deadline,
                "the creator's grant did not land"
            );
            edge::testing::edge()
                .sleep(std::time::Duration::from_millis(200))
                .await;
        }
        while !creator_may(DocumentPermission::Delete).await {
            assert!(
                std::time::Instant::now() < deadline,
                "the creator cannot delete"
            );
            edge::testing::edge()
                .sleep(std::time::Duration::from_millis(200))
                .await;
        }

        let version = users.schema().version_id.clone();
        let mut update = composite(vec![genesis.0], 2, 1);
        if let CrdtDelta::Composite(payload) = &mut update.delta {
            payload.collection_version_id = version.clone();
        }
        let update = signed(&key, update, &*blocks).await;
        assert_eq!(
            merge(update.clone(), doc_id.clone()).await,
            MergeOutcome::Merged
        );
        let mut delete = composite(vec![update.0], 3, 2);
        if let CrdtDelta::Composite(payload) = &mut delete.delta {
            payload.collection_version_id = version;
        }
        let delete = signed(&key, delete, &*blocks).await;
        assert_eq!(merge(delete, doc_id.clone()).await, MergeOutcome::Merged);
        let owner = loop {
            match vera.get_doc_owner(&policy, "users", &doc_id).await {
                Ok(owner) => break owner,
                Err(_) if std::time::Instant::now() < deadline => {
                    edge::testing::edge()
                        .sleep(std::time::Duration::from_millis(200))
                        .await
                }
                Err(error) => panic!("owner read: {error}"),
            }
        };
        assert_eq!(
            owner,
            Some(actor),
            "a merged delete leaves the object registered"
        );
        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
