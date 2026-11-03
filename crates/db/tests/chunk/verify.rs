//! Who may fetch a chunk block with its signature: whoever may read the parent.

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
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

fn identity(did: &str) -> Identity {
    Identity::Authenticated(identity::Did::new(did).unwrap())
}

fn schemas() -> Vec<schema::CollectionVersion> {
    let mut schemas = query::parse_sdl(SDL).unwrap();
    schemas[0].policy = Some(schema::PolicyDescription::new("policy", "users"));
    schemas
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
    db.create_collections_atomic(schemas()).await.unwrap();
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
fn only_a_reader_of_the_parent_gets_a_signed_chunk_block() {
    workers::block_on(async {
        let (db, created, acp) = author().await;
        for chunk in &created.derived {
            let cid = &chunk.commit_cid;
            verify(&db, &acp, cid, &identity(OWNER)).await.unwrap();
            for denied in [identity(STRANGER), Identity::Anonymous] {
                let error = verify(&db, &acp, cid, &denied).await.unwrap_err();
                assert!(error.contains("missing permission"), "{error}");
            }
        }
    })
    .unwrap()
}

#[test]
fn a_deleted_parent_keeps_its_chunk_history_behind_the_same_gate() {
    workers::block_on(async {
        let (db, created, acp) = author().await;
        AutoCommitMutator::new(Arc::clone(&db))
            .delete("Article", &created.doc_id)
            .await
            .unwrap();
        for chunk in &created.derived {
            let cid = &chunk.commit_cid;
            verify(&db, &acp, cid, &identity(OWNER)).await.unwrap();
            let error = verify(&db, &acp, cid, &identity(STRANGER))
                .await
                .unwrap_err();
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
        peer.create_collections_atomic(schemas()).await.unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(peer.clone(), blockstore.clone());
        let source_blocks = DefraBlockstore::new(source.store().clone(), false);

        let chunks = peer.get_collection("Article__body_chunk").unwrap().unwrap();
        let chunk = &created.derived[0];
        let bytes = source_blocks.get(&chunk.commit_cid).await.unwrap().unwrap();
        let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
        for link in block.links.iter().flatten() {
            let data = source_blocks.get(&link.link).await.unwrap().unwrap();
            blockstore.put(&link.link, &data).await.unwrap();
        }
        let signature = block.signature.unwrap();
        let data = source_blocks.get(&signature).await.unwrap().unwrap();
        blockstore.put(&signature, &data).await.unwrap();
        blockstore.put(&chunk.commit_cid, &bytes).await.unwrap();
        let outcome = handler
            .handle_block(
                &chunk.commit_cid,
                &bytes,
                BlockMetadata::normal(
                    &chunk.doc_id.to_string(),
                    chunks.collection_id(),
                    OWNER,
                    Some("peer"),
                    false,
                ),
            )
            .await
            .unwrap();
        assert!(outcome.is_merged(), "{outcome:?}");

        let acp = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
        for caller in [identity(OWNER), Identity::Anonymous] {
            let error = verify(&peer, &acp, &chunk.commit_cid, &caller)
                .await
                .unwrap_err();
            assert!(error.contains("parent is not known here"), "{error}");
        }
    })
    .unwrap()
}
