//! Collection-level head coverage for the P2P head provider.
//!
//! `BranchableSync` serves whatever `get_collection_heads` returns. A
//! branchable collection that has committed documents must therefore expose
//! its collection heads — otherwise every sync response is empty and a peer
//! can never pull the collection.

use db::merge::head_provider::DbHeadProvider;
use db::AutoCommitMutator;
use db::DB;
use document::Document;
use p2p::sync::DocumentHeadProvider;
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use storage::corekv::{IterOptions, Store};
use storage::RegolithStore;

fn branchable_schema() -> CollectionVersion {
    CollectionVersion::new(
        "Transcript",
        "v1",
        "col-transcript",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "body", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
    .as_branchable()
}

/// Every raw store key that textually contains a collection-head segment, so
/// a failure shows where heads actually landed (or that none were written).
async fn raw_collection_head_keys(store: &Arc<RegolithStore>) -> Vec<String> {
    let txn = store.new_txn(true).await.unwrap();
    let mut iter = txn.iterator(IterOptions::new()).await.unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        let key = String::from_utf8_lossy(&pair.key).into_owned();
        if key.contains("/hc/") {
            keys.push(key);
        }
    }
    keys
}

#[test]
fn branchable_collection_writes_expose_collection_heads() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(branchable_schema()).await.unwrap();

        let mutator = AutoCommitMutator::new(db.clone());
        let transcript = db
            .get_collection("Transcript")
            .unwrap()
            .unwrap()
            .schema_arc();
        let mut doc = Document::new(transcript);
        doc.set("body", "first".to_string()).unwrap();
        mutator.create("Transcript", doc).await.unwrap();

        let raw = raw_collection_head_keys(&store).await;
        let provider = DbHeadProvider::new(db);
        let heads = provider
            .get_collection_heads("col-transcript")
            .await
            .unwrap();
        assert!(
            !heads.is_empty(),
            "a branchable collection with one committed document must expose \
         collection heads to BranchableSync; raw '/hc/' keys in the store: {raw:?}"
        );
    })
    .unwrap()
}

/// Without a batch collector, a signing-enabled write signs each block,
/// the branchable collection's block included.
#[test]
#[mantra_macros::req_test("go.shinzo.signing.per_block_without_batch")]
fn a_signed_write_without_a_collector_signs_the_collection_block() {
    use crypto::{Key, PrivateKey};
    use defra_core::signing::{set_signing_config, SigningConfig, SigningKeyType};
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(branchable_schema()).await.unwrap();
        let transcript = db
            .get_collection("Transcript")
            .unwrap()
            .unwrap()
            .schema_arc();
        let mut doc = Document::new(transcript);
        doc.set("body", "first".to_string()).unwrap();

        let key = crypto::generate_secp256k1().unwrap();
        let public_key = key.public_key();
        set_signing_config(Some(SigningConfig {
            key_type: SigningKeyType::Secp256k1,
            private_key_bytes: key.raw_owned(),
            public_key_bytes: public_key.raw_owned(),
            public_key_hex: hex::encode(public_key.raw()),
            remote_signer: None,
            signing_authorization: None,
        }));
        let created = AutoCommitMutator::new(db.clone())
            .create("Transcript", doc)
            .await;
        set_signing_config(None);
        let created = created.unwrap();
        let composite =
            defra_core::Block::from_dag_cbor(created.commit_block.as_ref().unwrap()).unwrap();
        assert!(composite.signature.is_some(), "{composite:?}");

        let heads = DbHeadProvider::new(db.clone())
            .get_collection_heads("col-transcript")
            .await
            .unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let blocks = txn.blockstore().unwrap();
        assert!(!heads.is_empty());
        for head in heads {
            let bytes = blocks
                .get(&head.to_bytes())
                .await
                .unwrap()
                .expect("collection block stored");
            let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
            assert!(
                block.signature.is_some(),
                "the collection block is signed: {block:?}"
            );
        }
    })
    .unwrap()
}

/// Each document created in a branchable collection links the collection
/// chain to the head before it.
#[test]
#[mantra_macros::req_test("go.shinzo.heads.within_txn")]
fn each_create_links_the_collection_chain_to_the_previous_head() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(branchable_schema()).await.unwrap();
        let transcript = db
            .get_collection("Transcript")
            .unwrap()
            .unwrap()
            .schema_arc();
        let provider = DbHeadProvider::new(db.clone());
        let mut previous = Vec::new();
        for body in ["first", "second", "third"] {
            let mut doc = Document::new(transcript.clone());
            doc.set("body", body.to_string()).unwrap();
            AutoCommitMutator::new(db.clone())
                .create("Transcript", doc)
                .await
                .unwrap();
            let heads = provider
                .get_collection_heads("col-transcript")
                .await
                .unwrap();
            assert_eq!(heads.len(), 1, "one collection head after {body}");
            let txn = db.new_txn(true).await.unwrap();
            let bytes = txn
                .blockstore()
                .unwrap()
                .get(&heads[0].to_bytes())
                .await
                .unwrap()
                .unwrap();
            let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
            assert_eq!(
                block.heads.clone().unwrap_or_default(),
                previous,
                "{body} links the head before it"
            );
            previous = heads;
        }
    })
    .unwrap()
}
