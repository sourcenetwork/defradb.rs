//! A chunk's encryption key is released to whoever may read the parent.

use std::sync::Arc;

use acp::{DocumentACP, LocalDocumentACP, MemoryAcpStore};
use db::{AutoCommitMutator, DbBlockDocIDResolver, DbDocCollectionLookup, DB};
use defra_core::encryption::{set_encryption_config, EncryptionConfig};
use document::Document;
use identity::Did;
use kms::{
    DefraKms, EncryptionCid, Error, KmsService, MemoryKeyStore, NacDacPolicy, RequestContext,
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
    let mut schemas =
        query::parse_sdl("type Article { title: String body: String @chunk(maxChars: 64) }")
            .unwrap();
    schemas[0].policy = Some(schema::PolicyDescription::new("policy", "users"));
    db.create_collections_atomic(schemas).await.unwrap();

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
        encrypt_fields: vec!["body".to_string()],
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
    let txn = node.db.new_txn(true).await.unwrap();
    let blockstore = txn.blockstore().unwrap();
    let mut cids = Vec::new();
    for chunk in &node.created.derived {
        let composite = defra_core::Block::from_dag_cbor(&chunk.commit_block).unwrap();
        let text = composite
            .links
            .unwrap()
            .into_iter()
            .find(|link| link.name == "text")
            .expect("the text field block");
        let bytes = blockstore
            .get(&text.link.to_bytes())
            .await
            .unwrap()
            .unwrap();
        cids.push(
            defra_core::Block::from_dag_cbor(&bytes)
                .unwrap()
                .encryption
                .expect("the chunk text's key"),
        );
    }
    cids
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
