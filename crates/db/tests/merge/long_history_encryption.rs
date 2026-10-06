use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, Encryption, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use storage::RegolithStore;

use super::merge_handler_tests::make_handler_with_schema_and_bus;

async fn put(blockstore: &DefraBlockstore<RegolithStore>, block: &Block) -> (Cid, Vec<u8>) {
    let cid = block.generate_cid().unwrap();
    let bytes = block.to_dag_cbor().unwrap();
    blockstore.put(&cid, &bytes).await.unwrap();
    (cid, bytes)
}

async fn stored_document(
    handler: &DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
    doc_id: &str,
) -> Document {
    let collection = handler
        .db()
        .find_collection_by_id("col-users")
        .unwrap()
        .unwrap();
    let txn = handler.db().new_txn(true).await.unwrap();
    let doc = collection
        .get_by_doc_id(
            &txn.datastore().unwrap(),
            &txn.systemstore().unwrap(),
            &DocID::from_string(doc_id).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    txn.force_discard().unwrap();
    doc
}

#[tokio::test]
async fn incomplete_encrypted_ancestor_retries_then_preserves_all_fields() {
    let (base, blockstore, _bus) = make_handler_with_schema_and_bus().await;
    let handler =
        DbMergeHandler::new_with_max_merge_depth(base.db().clone(), blockstore.clone(), 8);
    let mut initial = Document::new();
    initial.set("name", NormalValue::String("Alice".into()));
    initial.set("age", NormalValue::Int(20));
    let genesis = db::block::builder::build_blocks_from_document(&initial, "v1", &blockstore)
        .await
        .unwrap();
    let metadata = BlockMetadata::normal(&genesis.doc_id, "col-users", "peer", None, false);
    assert_eq!(
        handler
            .handle_block(&genesis.cid, &genesis.block, metadata.clone())
            .await
            .unwrap(),
        MergeOutcome::Merged
    );

    let key = [7u8; 32];
    let encryption = Encryption::new(key.to_vec());
    let encryption_cid = encryption.generate_cid().unwrap();
    let mut plaintext = Vec::new();
    ciborium::into_writer(&NormalValue::String("Bob".into()), &mut plaintext).unwrap();
    let (ciphertext, _) =
        crypto::encryption::aes::encrypt_aes(&plaintext, &key, &[], true).unwrap();
    let mut encrypted = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".into(),
            schema_version_id: "v1".into(),
            priority: 2,
            data: ciphertext,
        }),
        vec![],
        vec![],
    );
    encrypted.encryption = Some(encryption_cid);
    let (encrypted_cid, _) = put(&blockstore, &encrypted).await;
    let mut age = Vec::new();
    ciborium::into_writer(&NormalValue::Int(42), &mut age).unwrap();
    let clear = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "age".into(),
            schema_version_id: "v1".into(),
            priority: 2,
            data: age,
        }),
        vec![],
        vec![],
    );
    let (clear_cid, _) = put(&blockstore, &clear).await;
    let mut tip = (genesis.cid, genesis.block.to_vec());
    for priority in 2..=33 {
        let links = if priority == 2 {
            // Decode the readable field first to expose accidental partial writes.
            vec![
                DAGLink::new("age", clear_cid),
                DAGLink::new("name", encrypted_cid),
            ]
        } else {
            vec![]
        };
        tip = put(
            &blockstore,
            &Block::new(
                CrdtDelta::Composite(CompositeDeltaPayload {
                    schema_version_id: "v1".into(),
                    priority,
                    status: 1,
                }),
                vec![tip.0],
                links,
            ),
        )
        .await;
    }

    let waiting = MergeOutcome::retryable_skip("history encrypted fields are not yet readable");
    let mut paused = false;
    let mut yielded = false;
    for _ in 0..128 {
        let outcome = handler
            .handle_block(&tip.0, &tip.1, metadata.clone())
            .await
            .unwrap();
        if outcome == MergeOutcome::Yielded {
            yielded = true;
            continue;
        }
        assert_eq!(outcome, waiting);
        paused = true;
        break;
    }
    assert!(
        yielded && paused,
        "history must yield and then wait for decryption"
    );
    for _ in 0..2 {
        assert_eq!(
            handler
                .handle_block(&tip.0, &tip.1, metadata.clone())
                .await
                .unwrap(),
            waiting
        );
    }
    let stored = stored_document(&handler, &genesis.doc_id).await;
    assert_eq!(
        stored.get("name"),
        Some(&NormalValue::String("Alice".into()))
    );
    assert_eq!(stored.get("age"), Some(&NormalValue::Int(20)));

    // Supply the previously missing key metadata without changing any composite CID.
    blockstore
        .put(&encryption_cid, &encryption.to_dag_cbor().unwrap())
        .await
        .unwrap();
    let mut complete = false;
    for _ in 0..128 {
        let outcome = handler
            .handle_block(&tip.0, &tip.1, metadata.clone())
            .await
            .unwrap();
        if outcome == MergeOutcome::Yielded {
            continue;
        }
        assert_eq!(outcome, MergeOutcome::Merged);
        complete = true;
        break;
    }
    assert!(
        complete,
        "readable history must finish within bounded retries"
    );
    let stored = stored_document(&handler, &genesis.doc_id).await;
    assert_eq!(stored.get("name"), Some(&NormalValue::String("Bob".into())));
    assert_eq!(stored.get("age"), Some(&NormalValue::Int(42)));
    assert!(handler
        .handle_block(&tip.0, &tip.1, metadata)
        .await
        .unwrap()
        .is_terminal_skip());
}
