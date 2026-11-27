//! Who may fetch a signed chunk block: whoever may read the parent. Honest
//! nodes sign none, so only a forged one is ever asked for.

use std::sync::Arc;

use acp::{DocumentACP, Identity, LocalDocumentACP, MemoryAcpStore};
use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use crypto::{Key, PrivateKey};
use db::block::verify::authorized_signed_block_bytes;
use db::merge::merge_handler::DbMergeHandler;
use db::{AutoCommitMutator, DB};
use defra_core::merge::{BlockMetadata, MergeHandler};
use defra_core::signing::{set_signing_config, SigningConfig, SigningKeyType};
use document::{Document, NormalValue};
use query::mutator::{CreateResult, DocMutator};
use storage::RegolithStore;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const STRANGER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const SDL: &str = r#"
    type Article @policy(id: "policy", resource: "users") {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

fn identity(did: &str) -> Identity {
    Identity::Authenticated(identity::Did::new(did).unwrap())
}

fn sign_writes() {
    let key = crypto::generate_ed25519().unwrap();
    let public_key = key.public_key();
    set_signing_config(Some(SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: key.raw_owned(),
        public_key_bytes: public_key.raw_owned(),
        public_key_hex: hex::encode(public_key.raw()),
        remote_signer: None,
        signing_authorization: None,
    }));
}

async fn author() -> (Arc<DB<RegolithStore>>, CreateResult, Arc<LocalDocumentACP>) {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .unwrap();
    let collection = db.get_collection("Article").unwrap().unwrap();
    let mut doc = Document::new(collection.schema_arc());
    doc.set("title", "t").unwrap();
    doc.set(
        "body",
        NormalValue::String("First paragraph.\n\nSecond paragraph that is a little longer.".into()),
    )
    .unwrap();
    sign_writes();
    let created = AutoCommitMutator::new(Arc::clone(&db))
        .create("Article", doc)
        .await
        .unwrap();
    set_signing_config(None);
    assert!(!created.derived.is_empty());

    let acp = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
    acp.register_doc_object(
        &identity::Did::new(OWNER).unwrap(),
        "policy",
        "users",
        &created.doc_id.to_string(),
        &[],
    )
    .await
    .unwrap();
    (db, created, acp)
}

async fn verify(
    db: &Arc<DB<RegolithStore>>,
    acp: &Arc<LocalDocumentACP>,
    cid: &Cid,
    caller: &Identity,
) -> Result<(), String> {
    authorized_signed_block_bytes(db, acp.as_ref(), &cid.to_string(), caller)
        .await
        .map(|_| ())
}

#[test]
fn a_chunk_block_is_unsigned_so_its_id_is_the_same_on_every_node() {
    workers::block_on(async {
        let (db, created, _) = author().await;
        let blocks = DefraBlockstore::new(db.store().clone(), false);
        for chunk in &created.derived {
            let bytes = blocks.get(&chunk.commit_cid).await.unwrap().unwrap();
            let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
            assert!(block.signature.is_none());
        }
    })
    .unwrap()
}

/// A signed chunk of `parent`, stored on `db`. Honest nodes sign no chunk
/// block, so this is what a peer that ignores that would send.
async fn signed_chunk(db: &Arc<DB<RegolithStore>>, parent: &str) -> (Cid, String) {
    let chunks = db.get_collection("Article__body_chunk").unwrap().unwrap();
    let mut chunk = Document::new(chunks.schema_arc());
    let text = "First paragraph.\n\n";
    chunk
        .set("_sourceID", NormalValue::String(parent.into()))
        .unwrap();
    chunk.set("text", NormalValue::String(text.into())).unwrap();
    chunk
        .set(
            "hash",
            NormalValue::String(db::chunk::hash::text_hash(text)),
        )
        .unwrap();
    let key = crypto::generate_ed25519().unwrap();
    let public_key = key.public_key();
    let signing = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: key.raw_owned(),
        public_key_bytes: public_key.raw_owned(),
        public_key_hex: hex::encode(public_key.raw()),
        remote_signer: None,
        signing_authorization: None,
    };
    let txn = db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let short_id = db.next_doc_short_id().await.unwrap();
    let created = db::block::builder::write_document_blocks(
        &txn.blockstore().unwrap(),
        &txn.headstore().unwrap(),
        &chunk,
        chunks.schema(),
        db::block::builder::DocRef::new(chunks.resolved_root_id(), short_id),
        None,
        None,
        Some(&signing),
        None,
    )
    .await
    .unwrap();
    db::docid::map::set_doc_id_mapping(
        &systemstore,
        chunks.resolved_root_id(),
        short_id,
        &created.doc_id,
    )
    .await
    .unwrap();
    for cid in std::iter::once(&created.cid).chain(&created.field_cids) {
        db::docid::map::set_block_doc_id_mapping(&systemstore, &cid.to_string(), &created.doc_id)
            .await
            .unwrap();
    }
    chunk.set_id(document::DocID::from_string(&created.doc_id).unwrap());
    chunks
        .save_with_datastore(&txn.datastore().unwrap(), &chunk, short_id)
        .await
        .unwrap();
    drop(systemstore);
    txn.force_commit().await.unwrap();
    (created.cid, created.doc_id)
}

#[test]
fn a_chunk_delete_block_is_unsigned_too() {
    workers::block_on(async {
        let (db, created, _) = author().await;
        sign_writes();
        let deleted = AutoCommitMutator::new(Arc::clone(&db))
            .delete("Article", &created.doc_id)
            .await
            .unwrap();
        set_signing_config(None);
        let blocks = DefraBlockstore::new(db.store().clone(), false);
        assert!(!deleted.derived.is_empty());
        for chunk in &deleted.derived {
            let bytes = blocks.get(&chunk.commit_cid).await.unwrap().unwrap();
            let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
            assert!(block.signature.is_none());
        }
    })
    .unwrap()
}

#[test]
fn only_a_reader_of_the_parent_gets_a_signed_chunk_block() {
    workers::block_on(async {
        let (db, created, acp) = author().await;
        let (cid, _) = signed_chunk(&db, &created.doc_id.to_string()).await;

        verify(&db, &acp, &cid, &identity(OWNER)).await.unwrap();
        for denied in [identity(STRANGER), Identity::Anonymous] {
            let error = verify(&db, &acp, &cid, &denied).await.unwrap_err();
            assert!(error.contains("missing permission"), "{error}");
        }
    })
    .unwrap()
}

#[test]
fn a_chunk_block_whose_parent_has_not_merged_is_refused_to_everyone() {
    workers::block_on(async {
        let (source, created, _) = author().await;
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peer = Arc::new(DB::from_arc(store.clone()).unwrap());
        peer.add_collection(&defra_core::RequestContext::ambient(), SDL)
            .await
            .unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(peer.clone(), blockstore.clone());
        let source_blocks = DefraBlockstore::new(source.store().clone(), false);
        let (cid, doc_id) = signed_chunk(&source, &created.doc_id.to_string()).await;

        let chunks = peer.get_collection("Article__body_chunk").unwrap().unwrap();
        let bytes = source_blocks.get(&cid).await.unwrap().unwrap();
        let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
        let mut copies: Vec<Cid> = block.signature.into_iter().collect();
        for link in block.links.iter().flatten() {
            let data = source_blocks.get(&link.link).await.unwrap().unwrap();
            copies.extend(defra_core::Block::from_dag_cbor(&data).unwrap().signature);
            blockstore.put(&link.link, &data).await.unwrap();
        }
        for signature in copies {
            let data = source_blocks.get(&signature).await.unwrap().unwrap();
            blockstore.put(&signature, &data).await.unwrap();
        }
        blockstore.put(&cid, &bytes).await.unwrap();
        let outcome = handler
            .handle_block(
                &cid,
                &bytes,
                BlockMetadata::normal(&doc_id, chunks.collection_id(), OWNER, Some("peer"), false),
            )
            .await
            .unwrap();
        assert!(outcome.is_merged(), "{outcome:?}");

        let acp = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
        for caller in [identity(OWNER), Identity::Anonymous] {
            let error = verify(&peer, &acp, &cid, &caller).await.unwrap_err();
            assert!(error.contains("parent is not known here"), "{error}");
        }
    })
    .unwrap()
}
