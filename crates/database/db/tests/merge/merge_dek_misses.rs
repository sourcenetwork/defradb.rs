//! A merge fetches the decryption keys its blocks need outside its
//! engine-owned attempts, all in one request per authorization context, and
//! never asks for a key again. A document with more encrypted fields than any
//! fixed count merges whole.

use async_trait::async_trait;
use blockstore::Blockstore as _;
use blockstore::DefraBlockstore;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, Encryption, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{DocID, NormalValue};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use storage::RegolithStore;

const FIELDS: usize = 80;

/// Serves the keys it was given and counts every request.
struct KeyServer {
    keys: HopscotchMap<kms::EncryptionCid, [u8; 32], RandomState>,
    requests: AtomicUsize,
    max_batch: AtomicUsize,
}

#[async_trait]
impl kms::KmsService for KeyServer {
    async fn get_keys(
        &self,
        _: &kms::RequestContext,
        cids: &[kms::EncryptionCid],
    ) -> kms::Result<kms::KeyResults> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.max_batch.fetch_max(cids.len(), Ordering::SeqCst);
        let (results, sender) = kms::KeyResults::new(cids.len());
        for cid in cids {
            let key = self.keys.get(cid).ok_or(kms::Error::KeyUnavailable)?;
            sender.send(Ok((*cid, key))).await.unwrap();
        }
        drop(sender);
        Ok(results)
    }

    async fn generate_key(
        &self,
        _: &kms::RequestContext,
        _: kms::KeyScope,
    ) -> kms::Result<(kms::EncryptionCid, [u8; 32])> {
        Err(kms::Error::Unsupported("merges generate no keys"))
    }

    async fn serve_request(
        &self,
        _: kms::PeerIdentity,
        _: kms::FetchEncryptionKeyRequest,
    ) -> kms::Result<kms::FetchEncryptionKeyReply> {
        Err(kms::Error::Unsupported("unexpected peer request"))
    }
}

fn field_name(index: usize) -> String {
    format!("f{index}")
}

#[test]
fn a_merge_with_more_encrypted_fields_than_any_cap_merges_whole() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        let mut fields = vec![FieldDescription::new(
            "1",
            "_docID",
            FieldKind::doc_id(),
            schema::CType::LwwRegister,
        )];
        fields.extend((0..FIELDS).map(|index| {
            FieldDescription::new(
                (index + 2).to_string(),
                field_name(index),
                FieldKind::string(),
                schema::CType::LwwRegister,
            )
        }));
        db.create_collection(CollectionVersion::new("Wide", "v1", "col-wide", fields))
            .await
            .unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
        let server = Arc::new(KeyServer {
            keys: HopscotchMap::with_hasher(RandomState::default()),
            requests: AtomicUsize::new(0),
            max_batch: AtomicUsize::new(0),
        });

        let mut links = Vec::with_capacity(FIELDS);
        for index in 0..FIELDS {
            let mut key = [0_u8; 32];
            key[..8].copy_from_slice(&(index as u64 + 1).to_be_bytes());
            let encryption_cid = Encryption::new(key.to_vec()).generate_cid().unwrap();
            server.keys.insert(encryption_cid, key);
            let mut plaintext = Vec::new();
            ciborium::into_writer(
                &NormalValue::String(format!("value {index}")),
                &mut plaintext,
            )
            .unwrap();
            let (ciphertext, _) =
                crypto::encryption::aes::encrypt_aes(&plaintext, &key, &[], true).unwrap();
            let field = Block::new_with_options(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: field_name(index),
                    collection_version_id: "v1".into(),
                    priority: 1,
                    data: ciphertext,
                }),
                vec![],
                vec![],
                Some(encryption_cid),
                None,
            );
            let field_cid = field.generate_cid().unwrap();
            blockstore
                .put(&field_cid, &field.to_dag_cbor().unwrap())
                .await
                .unwrap();
            links.push(DAGLink::new(field_name(index), field_cid));
        }
        let composite = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "v1".into(),
                priority: 1,
                status: 1,
            }),
            vec![],
            links,
        );
        let composite_cid = composite.generate_cid().unwrap();
        let doc_id = db::block::builder::derive_doc_id(&composite_cid);
        let metadata =
            BlockMetadata::normal(&doc_id, "col-wide", "did:key:z6MkrWideMerge", None, false);

        handler.set_kms(server.clone());
        let outcome = handler
            .handle_block(&composite_cid, &composite.to_dag_cbor().unwrap(), metadata)
            .await
            .unwrap();
        assert_eq!(outcome, MergeOutcome::Merged);
        // One request for every key, before the merge's transaction: a key once
        // fetched is never asked for again.
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);

        let collection = db.find_collection_by_id("col-wide").unwrap().unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let doc_ref = db::docid::map::get_doc_ref(&txn.systemstore().unwrap(), &doc_id)
            .await
            .unwrap()
            .expect("the merge registers the document");
        let stored = collection
            .get_with_datastore_include_deleted(
                &txn.datastore().unwrap(),
                doc_ref.doc_short_id,
                &DocID::from_string(&doc_id).unwrap(),
                false,
            )
            .await
            .unwrap()
            .expect("the merge materializes the document")
            .0;
        for index in 0..FIELDS {
            assert_eq!(
                stored.get(&field_name(index)),
                Some(&NormalValue::String(format!("value {index}")))
            );
        }
    })
    .unwrap()
}

#[test]
fn history_pages_batch_their_encryption_keys() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(CollectionVersion::new(
            "History",
            "v1",
            "col-history",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            ],
        ))
        .await
        .unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new_with_max_merge_depth(db.clone(), blockstore.clone(), 8);
        let server = Arc::new(KeyServer {
            keys: HopscotchMap::with_hasher(RandomState::default()),
            requests: AtomicUsize::new(0),
            max_batch: AtomicUsize::new(0),
        });
        let mut root = None;
        let mut doc_id = None;
        let mut field_parent = None;
        for priority in 1_u64..=32 {
            let mut key = [0; 32];
            key[..8].copy_from_slice(&priority.to_be_bytes());
            let encryption_cid = Encryption::new(key.to_vec()).generate_cid().unwrap();
            server.keys.insert(encryption_cid, key);
            let mut plaintext = Vec::new();
            ciborium::into_writer(
                &NormalValue::String(format!("value-{priority}")),
                &mut plaintext,
            )
            .unwrap();
            let (data, _) =
                crypto::encryption::aes::encrypt_aes(&plaintext, &key, &[], true).unwrap();
            let field = Block::new_with_options(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: "name".into(),
                    collection_version_id: "v1".into(),
                    priority,
                    data,
                }),
                field_parent.into_iter().collect(),
                vec![],
                Some(encryption_cid),
                None,
            );
            let field_cid = field.generate_cid().unwrap();
            blockstore
                .put(&field_cid, &field.to_dag_cbor().unwrap())
                .await
                .unwrap();
            let composite = Block::new(
                CrdtDelta::Composite(CompositeDeltaPayload {
                    collection_version_id: "v1".into(),
                    priority,
                    status: 1,
                }),
                root.into_iter().collect(),
                vec![DAGLink::new("name", field_cid)],
            );
            let cid = composite.generate_cid().unwrap();
            blockstore
                .put(&cid, &composite.to_dag_cbor().unwrap())
                .await
                .unwrap();
            doc_id.get_or_insert_with(|| db::block::builder::derive_doc_id(&cid));
            field_parent = Some(field_cid);
            root = Some(cid);
        }
        let root = root.unwrap();
        let doc_id = doc_id.unwrap();
        handler.set_kms(server.clone());
        let bytes = blockstore.get(&root).await.unwrap().unwrap();
        assert_eq!(
            handler
                .handle_block(
                    &root,
                    &bytes,
                    BlockMetadata::normal(&doc_id, "col-history", "peer", None, false),
                )
                .await
                .unwrap(),
            MergeOutcome::Merged
        );
        assert!(server.max_batch.load(Ordering::SeqCst) > 1);
        assert!(
            server.requests.load(Ordering::SeqCst) <= 8,
            "keys should be fetched per page, not per revision"
        );
        let collection = db.find_collection_by_id("col-history").unwrap().unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let document = collection
            .get_by_doc_id(
                &txn.datastore().unwrap(),
                &txn.systemstore().unwrap(),
                &DocID::from_string(&doc_id).unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            document.get("name"),
            Some(&NormalValue::String("value-32".into()))
        );
    })
    .unwrap()
}
