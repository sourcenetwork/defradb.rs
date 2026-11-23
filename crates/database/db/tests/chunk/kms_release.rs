//! A chunk's encryption key is released to whoever may read the parent.

use std::sync::Arc;

use acp::{DocumentACP, LocalDocumentACP, MemoryAcpStore};
use db::{AutoCommitMutator, DbBlockDocIDResolver, DbDocCollectionLookup, DB};
use defra_core::encryption::{set_encryption_config, EncryptionConfig};
use defra_core::merge::ExplicitReplayAuthorization;
use document::Document;
use identity::Did;
use kms::{
    DefraKms, EncryptionCid, Error, FetchEncryptionKeyRequest, KmsService, MemoryKeyStore,
    NacDacPolicy, PeerIdentity, RequestContext,
};
use query::mutator::{CreateResult, DocMutator};
use storage::RegolithStore;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const STRANGER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

struct Node {
    db: Arc<DB<RegolithStore>>,
    kms: Arc<DefraKms>,
    created: CreateResult,
}

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

async fn node() -> Node {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.add_collection(
        &defra_core::RequestContext::ambient(),
        r#"type Article @policy(id: "policy", resource: "users") { title: String body: String @chunk(maxChars: 64) }"#,
    )
    .await
    .unwrap();

    let acp = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
    let policy = Arc::new(NacDacPolicy::new(
        acp.clone(),
        Arc::new(DbDocCollectionLookup::new(Arc::clone(&db))),
    ));
    let kms = Arc::new(DefraKms::new(
        Arc::new(MemoryKeyStore::new()),
        Vec::new(),
        policy,
        Arc::new(DbBlockDocIDResolver::new(Arc::clone(&db))),
        did(OWNER),
    ));
    db.set_kms(kms.clone());

    let collection = db.get_collection("Article").unwrap().unwrap();
    let mut doc = Document::new(collection.schema_arc());
    doc.set("title", "t").unwrap();
    doc.set(
        "body",
        "First paragraph.\n\nSecond paragraph that is a little longer.",
    )
    .unwrap();
    set_encryption_config(Some(EncryptionConfig {
        encrypt_doc: false,
        encrypt_fields: vec!["title".to_string(), "body".to_string()],
    }));
    let created = AutoCommitMutator::new(Arc::clone(&db))
        .create("Article", doc)
        .await
        .unwrap();
    set_encryption_config(None);
    assert!(!created.derived.is_empty());

    acp.register_doc_object(&did(OWNER), "policy", "users", &created.doc_id.to_string())
        .await
        .unwrap();
    Node { db, kms, created }
}

async fn chunk_key_cids(node: &Node) -> Vec<EncryptionCid> {
    let mut cids = Vec::new();
    for chunk in &node.created.derived {
        cids.push(field_key_cid(node, &chunk.commit_block, "text").await);
    }
    cids
}

async fn title_key_cid(node: &Node) -> EncryptionCid {
    let composite = node
        .created
        .commit_block
        .as_ref()
        .expect("the article's block");
    field_key_cid(node, composite, "title").await
}

/// The key CID of `field`'s block under the composite block `composite`.
async fn field_key_cid(node: &Node, composite: &[u8], field: &str) -> EncryptionCid {
    let link = defra_core::Block::from_dag_cbor(composite)
        .unwrap()
        .links
        .unwrap()
        .into_iter()
        .find(|link| link.name == field)
        .expect("the field block");
    let txn = node.db.new_txn(true).await.unwrap();
    let bytes = txn
        .blockstore()
        .unwrap()
        .get(&link.link.to_bytes())
        .await
        .unwrap()
        .unwrap();
    let _ = txn.discard();
    defra_core::Block::from_dag_cbor(&bytes)
        .unwrap()
        .encryption
        .expect("the field's key")
}

fn collection_id(node: &Node, name: &str) -> String {
    node.db
        .get_collection(name)
        .unwrap()
        .unwrap()
        .collection_id()
        .to_string()
}

/// Whether a peer replaying for `OWNER` under a delegation on `collection_id` gets `cid`.
async fn delegate_gets(node: &Node, collection_id: &str, cid: EncryptionCid) -> bool {
    // The x25519 base point: any valid public key will do.
    let mut ephemeral_public_key = vec![0u8; 32];
    ephemeral_public_key[0] = 9;
    let request = FetchEncryptionKeyRequest {
        identity: STRANGER.as_bytes().to_vec(),
        links: vec![cid.to_bytes()],
        ephemeral_public_key,
        explicit_replay_capability: Some("capability".into()),
    };
    let from = PeerIdentity {
        peer_id: "target-peer".into(),
        authenticated_did: Some(did(STRANGER)),
        explicit_replay_authorization: Some(ExplicitReplayAuthorization {
            source_peer_id: "source-peer".into(),
            target_peer_id: "target-peer".into(),
            collection_id: collection_id.into(),
            authorizer_did: OWNER.into(),
            expires_at: u64::MAX,
            capability: Some("capability".into()),
        }),
    };
    let reply = node.kms.serve_request(from, request).await.unwrap();
    reply.links == vec![cid.to_bytes()]
}

async fn release(node: &Node, actor: Option<&str>, cid: EncryptionCid) -> Result<[u8; 32], Error> {
    let context = match actor {
        Some(actor) => RequestContext::with_user(did(actor)),
        None => RequestContext::anonymous(),
    };
    let results = node.kms.get_keys(&context, &[cid]).await.unwrap();
    results.wait_all().await.map(|keys| keys[&cid])
}

#[test]
fn a_chunk_key_goes_to_a_reader_of_the_parent_only() {
    workers::block_on(async {
        let node = node().await;
        for cid in chunk_key_cids(&node).await {
            release(&node, Some(OWNER), cid).await.unwrap();
            for denied in [Some(STRANGER), None] {
                let error = release(&node, denied, cid).await.unwrap_err();
                assert!(matches!(error, Error::AccessDenied { .. }), "{error:?}");
            }
        }
    })
    .unwrap()
}

#[test]
fn a_deleted_parents_chunk_keys_stay_behind_the_same_gate() {
    workers::block_on(async {
        let node = node().await;
        AutoCommitMutator::new(Arc::clone(&node.db))
            .delete("Article", &node.created.doc_id)
            .await
            .unwrap();
        for cid in chunk_key_cids(&node).await {
            release(&node, Some(OWNER), cid).await.unwrap();
            let error = release(&node, Some(STRANGER), cid).await.unwrap_err();
            assert!(matches!(error, Error::AccessDenied { .. }), "{error:?}");
        }
    })
    .unwrap()
}

#[test]
fn a_delegation_on_the_chunk_collection_gets_chunk_keys_only() {
    workers::block_on(async {
        let node = node().await;
        let chunks = collection_id(&node, "Article__body_chunk");
        for cid in chunk_key_cids(&node).await {
            assert!(delegate_gets(&node, &chunks, cid).await, "a chunk key");
        }
        assert!(
            !delegate_gets(&node, &chunks, title_key_cid(&node).await).await,
            "the article's title key is outside a chunk delegation"
        );
    })
    .unwrap()
}

#[test]
fn a_delegation_on_the_parent_gets_its_own_and_its_chunks_keys() {
    workers::block_on(async {
        let node = node().await;
        let article = collection_id(&node, "Article");
        assert!(delegate_gets(&node, &article, title_key_cid(&node).await).await);
        for cid in chunk_key_cids(&node).await {
            assert!(delegate_gets(&node, &article, cid).await, "a chunk key");
        }
    })
    .unwrap()
}
