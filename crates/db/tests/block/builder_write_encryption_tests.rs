//! Encryption-derivation behaviour for `write_document_blocks`.
//!
//! Split out of `write.rs` to keep that file within the repo's size guidance.

use cid::Cid;
use datastore::NamespaceView;
use datastore::SharedTxn;
use db::block::builder::encode_value_as_cbor;
use db::block::builder::*;
use defra_core::block::Block;
use defra_core::block::CrdtDelta;
use defra_core::encryption::EncryptionConfig;
use document::Document;
use document::NormalValue;
use rapidhash::RapidHashSet;
use storage::corekv::Store;
use storage::namespace::Namespace;
use storage::RegolithStore;

async fn stores() -> (NamespaceView, NamespaceView) {
    let store = RegolithStore::in_memory().unwrap();
    let txn = store.new_txn(false).await.unwrap();
    let shared = SharedTxn::new(txn);
    (
        NamespaceView::new(shared.clone(), Namespace::Blockstore),
        NamespaceView::new(shared, Namespace::Headstore),
    )
}

fn delta_data(block: &Block) -> Vec<u8> {
    match &block.delta {
        CrdtDelta::Lww(payload) => payload.data.clone(),
        other => panic!("expected an LWW delta, got {:?}", other),
    }
}

async fn field_block(blockstore: &NamespaceView, cid: &Cid) -> Block {
    let bytes = blockstore
        .get(&cid.to_bytes())
        .await
        .unwrap()
        .expect("block stored");
    Block::from_dag_cbor(&bytes).unwrap()
}

/// An update that carries no encryption config must still be encrypted when
/// the field's previous block was encrypted, by inheriting that block's
/// encryption link. Mirrors Go's `determineBlockEncryption`
/// (internal/core/block/store.go) and its
/// `TestDocEncryption_UponUpdateOnLWWCRDT_ShouldEncryptCommitDelta`.
#[test]
fn update_without_config_inherits_encryption_from_previous_block() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(1, 1);

        let mut doc = Document::new(crate::common::schema::open_collection(&["secret"]));
        doc.set("secret", NormalValue::String("classified".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".to_string()],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");

        let created_enc_cid = field_block(&blockstore, &created.field_cids[0])
            .await
            .encryption
            .expect("created field block must be encrypted");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set(
            "secret",
            NormalValue::String("still classified".to_string()),
        )
        .unwrap();
        let modified: RapidHashSet<String> = ["secret".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        assert_eq!(
            field_block(&blockstore, &updated.field_cids[0])
                .await
                .encryption,
            Some(created_enc_cid),
            "update must inherit the previous block's encryption link, not write plaintext"
        );
    })
    .unwrap()
}

/// The composite block carries its own encryption link for whole-document
/// encryption, and must inherit it on update for the same reason.
#[test]
fn update_without_config_inherits_composite_encryption() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(2, 2);

        let mut doc = Document::new(crate::common::schema::open_collection(&["name"]));
        doc.set("name", NormalValue::String("Alice".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: vec![],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");

        let created_enc_cid = field_block(&blockstore, &created.cid)
            .await
            .encryption
            .expect("created composite block must be encrypted");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set("name", NormalValue::String("Alicia".to_string()))
            .unwrap();
        let modified: RapidHashSet<String> = ["name".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        assert_eq!(
            field_block(&blockstore, &updated.cid).await.encryption,
            Some(created_enc_cid),
            "composite block must inherit the previous composite block's encryption link"
        );
    })
    .unwrap()
}

/// Inheriting reuses the existing `Encryption` block rather than writing a
/// new one, matching Go's `determineBlockEncryption`, which returns the
/// previous link untouched.
#[test]
fn inherited_encryption_writes_no_new_encryption_block() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(4, 4);

        let mut doc = Document::new(crate::common::schema::open_collection(&["secret"]));
        doc.set("secret", NormalValue::String("classified".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".to_string()],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");
        assert_eq!(
            created.encryption_cids.len(),
            1,
            "create must mint one encryption block"
        );

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set(
            "secret",
            NormalValue::String("still classified".to_string()),
        )
        .unwrap();
        let modified: RapidHashSet<String> = ["secret".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        assert!(
            updated.encryption_cids.is_empty(),
            "inheriting must not mint a new encryption block"
        );
    })
    .unwrap()
}

/// An explicit config on the update takes precedence over what the heads
/// say, and rotates the key.
#[test]
fn explicit_config_on_update_overrides_inheritance() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(5, 5);

        let mut doc = Document::new(crate::common::schema::open_collection(&["secret"]));
        doc.set("secret", NormalValue::String("classified".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".to_string()],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");
        let created_enc_cid = field_block(&blockstore, &created.field_cids[0])
            .await
            .encryption
            .expect("created field block must be encrypted");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set(
            "secret",
            NormalValue::String("still classified".to_string()),
        )
        .unwrap();
        let modified: RapidHashSet<String> = ["secret".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("update should succeed");

        let updated_enc_cid = field_block(&blockstore, &updated.field_cids[0])
            .await
            .encryption
            .expect("explicitly configured update must be encrypted");
        assert_ne!(
            updated_enc_cid, created_enc_cid,
            "an explicit config must mint a fresh key rather than inherit"
        );
        assert_eq!(
            updated.encryption_cids.len(),
            1,
            "the explicit path must record the encryption block it wrote"
        );
    })
    .unwrap()
}

/// Inheritance is per field: a field that was never encrypted does not
/// become encrypted because a sibling field was.
#[test]
fn inheritance_is_per_field() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(6, 6);

        let mut doc = Document::new(crate::common::schema::open_collection(&[
            "secret", "public",
        ]));
        doc.set("secret", NormalValue::String("classified".to_string()))
            .unwrap();
        doc.set("public", NormalValue::String("open".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".to_string()],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set(
            "secret",
            NormalValue::String("still classified".to_string()),
        )
        .unwrap();
        doc.set("public", NormalValue::String("still open".to_string()))
            .unwrap();
        let modified: RapidHashSet<String> = ["secret".to_string(), "public".to_string()]
            .into_iter()
            .collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        let mut encrypted = 0;
        for cid in &updated.field_cids {
            if field_block(&blockstore, cid).await.encryption.is_some() {
                encrypted += 1;
            }
        }
        assert_eq!(
            (updated.field_cids.len(), encrypted),
            (2, 1),
            "exactly the previously-encrypted field should inherit encryption"
        );
    })
    .unwrap()
}

/// A field first written by an update has no heads of its own, so field-level
/// inheritance finds nothing. On a document encrypted as a whole it must still
/// be encrypted, under the document-level policy the composite head records.
///
/// **Deliberate divergence from Go.** Go's `addDelta` builds its head set from
/// the field's own prefix (`internal/core/block/store.go:82-86`,
/// `NewHeadSet(txn.Headstore(), crdtData.HeadstorePrefix())`), so
/// `determineBlockEncryption` sees no heads and writes the new field in
/// plaintext. Matching that would regress our own pre-#1292 behaviour, which
/// encrypted this field, and would reopen the silent-plaintext hazard this
/// derivation exists to close.
#[test]
fn field_added_by_update_inherits_document_encryption() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(7, 7);

        let mut doc = Document::new(crate::common::schema::open_collection(&["name", "bio"]));
        doc.set("name", NormalValue::String("Alice".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: vec![],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set("bio", NormalValue::String("ssn 123-45-6789".to_string()))
            .unwrap();
        let modified: RapidHashSet<String> = ["bio".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        assert_eq!(updated.field_cids.len(), 1, "only `bio` should get a block");
        let bio = field_block(&blockstore, &updated.field_cids[0]).await;
        assert!(
            bio.encryption.is_some(),
            "a field introduced on an `encrypt_doc` document must be encrypted \
         under the document-level policy, not written in plaintext"
        );
        assert_ne!(
            delta_data(&bio),
            encode_value_as_cbor(&NormalValue::String("ssn 123-45-6789".to_string())).unwrap(),
            "the delta must hold ciphertext, not the plaintext value"
        );
    })
    .unwrap()
}

/// Derivation must not invent encryption: a document created in the clear
/// stays in the clear.
#[test]
fn update_of_unencrypted_document_stays_plaintext() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(3, 3);

        let mut doc = Document::new(crate::common::schema::open_collection(&["name"]));
        doc.set("name", NormalValue::String("Bob".to_string()))
            .unwrap();

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("create should succeed");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set("name", NormalValue::String("Bobby".to_string()))
            .unwrap();
        let modified: RapidHashSet<String> = ["name".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        assert_eq!(
            field_block(&blockstore, &updated.field_cids[0])
                .await
                .encryption,
            None,
            "an unencrypted document must not become encrypted by derivation"
        );
        assert_eq!(
            field_block(&blockstore, &updated.cid).await.encryption,
            None
        );
    })
    .unwrap()
}

/// Deleting an encrypted document must leave a tombstone carrying the same
/// `Encryption` link as the composite it supersedes. Go's delete path runs
/// through the same `addDelta` as every other write
/// (`internal/db/document_delete.go:181-195`), so `determineBlockEncryption`
/// inherits from the composite heads. The composite payload itself is never
/// ciphertext (`store.go:213-215`); only the link is attached.
#[test]
fn delete_block_inherits_composite_encryption() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(8, 8);

        let mut doc = Document::new(crate::common::schema::open_collection(&["name"]));
        doc.set("name", NormalValue::String("Alice".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: vec![],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");

        let created_enc_cid = field_block(&blockstore, &created.cid)
            .await
            .encryption
            .expect("created composite must carry an encryption link");

        let deleted = write_delete_block(
            &blockstore,
            &headstore,
            &created.doc_id,
            8,
            "schema-v1",
            None,
        )
        .await
        .expect("delete should succeed");

        assert_eq!(
            field_block(&blockstore, &deleted.cid).await.encryption,
            Some(created_enc_cid),
            "delete tombstone must inherit the composite's encryption link"
        );
    })
    .unwrap()
}

/// The document-level fallback must key off whole-document encryption only.
/// A document with `encrypt_fields` leaves its composite unencrypted, so a
/// field that was never in that list stays in the clear when introduced later.
#[test]
fn field_added_to_field_encrypted_document_stays_plaintext() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(9, 9);

        let mut doc = Document::new(crate::common::schema::open_collection(&["secret", "bio"]));
        doc.set("secret", NormalValue::String("classified".to_string()))
            .unwrap();

        let enc = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".to_string()],
        };

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("create should succeed");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set("bio", NormalValue::String("public".to_string()))
            .unwrap();
        let modified: RapidHashSet<String> = ["bio".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        assert_eq!(
            field_block(&blockstore, &updated.field_cids[0])
                .await
                .encryption,
            None,
            "`bio` was never in `encrypt_fields`, so nothing should encrypt it"
        );
    })
    .unwrap()
}

/// Derivation must not invent encryption for a field either: a document
/// created in the clear stays in the clear when a field is added to it.
#[test]
fn field_added_to_unencrypted_document_stays_plaintext() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(10, 10);

        let mut doc = Document::new(crate::common::schema::open_collection(&["name", "bio"]));
        doc.set("name", NormalValue::String("Alice".to_string()))
            .unwrap();

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("create should succeed");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
        doc.set("bio", NormalValue::String("public".to_string()))
            .unwrap();
        let modified: RapidHashSet<String> = ["bio".to_string()].into_iter().collect();

        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        assert_eq!(
            field_block(&blockstore, &updated.field_cids[0])
                .await
                .encryption,
            None,
            "an unencrypted document must not acquire encryption from a new field"
        );
    })
    .unwrap()
}

/// The fallback is not gated on the field being brand new. A field whose
/// history predates the document being encrypted has heads, but none of them
/// carry an encryption link, so field-level inheritance finds nothing — the
/// document-level policy must still cover it.
#[test]
fn document_policy_covers_field_whose_history_predates_encryption() {
    workers::block_on(async {
        let (blockstore, headstore) = stores().await;
        let identity = DocRef::new(11, 11);

        let mut doc = Document::new(crate::common::schema::open_collection(&["name", "bio"]));
        doc.set("name", NormalValue::String("Alice".to_string()))
            .unwrap();
        doc.set("bio", NormalValue::String("public".to_string()))
            .unwrap();

        let created = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("create should succeed");

        doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());

        // Turn on whole-document encryption while touching only `bio`.
        doc.set("bio", NormalValue::String("still public".to_string()))
            .unwrap();
        let enc = EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: vec![],
        };
        write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&["bio".to_string()].into_iter().collect()),
            Some(&enc),
            None,
            None,
        )
        .await
        .expect("encrypting update should succeed");

        // `name` still has only its unencrypted create block as a head.
        doc.set("name", NormalValue::String("Bob".to_string()))
            .unwrap();
        let updated = write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &crate::common::schema::version("schema-v1"),
            identity,
            Some(&["name".to_string()].into_iter().collect()),
            None,
            None,
            None,
        )
        .await
        .expect("update should succeed");

        let name = field_block(&blockstore, &updated.field_cids[0]).await;
        assert!(
            name.encryption.is_some(),
            "the document is encrypted as a whole, so `name` must be too"
        );
        assert_ne!(
            delta_data(&name),
            encode_value_as_cbor(&NormalValue::String("Bob".to_string())).unwrap(),
            "the delta must hold ciphertext, not the plaintext value"
        );
    })
    .unwrap()
}

fn immutable_schema() -> schema::CollectionVersion {
    use schema::{FieldDescription, FieldKind};
    schema::CollectionVersion::new(
        "Records",
        "v1",
        "col-records",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "author",
                FieldKind::string(),
                schema::CType::LwwRegister,
            )
            .as_immutable(),
            FieldDescription::new("3", "kind", FieldKind::string(), schema::CType::LwwRegister)
                .as_immutable(),
        ],
    )
}

/// Create a record under `enc` and return the composite's links by name.
async fn create_immutable_record(
    enc: &EncryptionConfig,
) -> (NamespaceView, std::collections::BTreeMap<String, Cid>) {
    let (blockstore, headstore) = stores().await;
    let mut doc = Document::new(crate::common::schema::open_collection(&["author", "kind"]));
    doc.set("author", NormalValue::String("did:key:alice".to_string()))
        .unwrap();
    doc.set("kind", NormalValue::String("note".to_string()))
        .unwrap();

    let created = write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        &immutable_schema(),
        DocRef::new(11, 11),
        None,
        Some(enc),
        None,
        None,
    )
    .await
    .expect("create should succeed");

    let links = Block::from_dag_cbor(&created.block)
        .unwrap()
        .links
        .unwrap_or_default()
        .into_iter()
        .map(|link| (link.name, link.link))
        .collect();
    (blockstore, links)
}

/// An encrypted `@immutable` field keeps its own encrypted block: the
/// Immutable block holds plaintext, so it carries only the fields left clear.
#[test]
fn encrypted_immutable_field_stays_out_of_the_immutable_block() {
    workers::block_on(async {
        let enc = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["author".to_string()],
        };
        let (blockstore, links) = create_immutable_record(&enc).await;

        let immutable =
            field_block(&blockstore, &links[defra_core::block::IMMUTABLE_LINK_NAME]).await;
        let CrdtDelta::Immutable(payload) = immutable.delta else {
            panic!("expected an Immutable block, got {:?}", immutable.delta);
        };
        assert_eq!(payload.fields.keys().collect::<Vec<_>>(), ["kind"]);

        let author = field_block(&blockstore, &links["author"]).await;
        assert!(author.encryption.is_some(), "`author` must be encrypted");
        assert_ne!(
            delta_data(&author),
            encode_value_as_cbor(&NormalValue::String("did:key:alice".to_string())).unwrap()
        );
    })
    .unwrap()
}

/// Whole-document encryption leaves no field in the clear, so no Immutable
/// block is written.
#[test]
fn document_encryption_writes_no_immutable_block() {
    workers::block_on(async {
        let enc = EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: vec![],
        };
        let (blockstore, links) = create_immutable_record(&enc).await;

        assert!(!links.contains_key(defra_core::block::IMMUTABLE_LINK_NAME));
        for (name, plaintext) in [("author", "did:key:alice"), ("kind", "note")] {
            let block = field_block(&blockstore, &links[name]).await;
            assert!(block.encryption.is_some(), "`{name}` must be encrypted");
            assert_ne!(
                delta_data(&block),
                encode_value_as_cbor(&NormalValue::String(plaintext.to_string())).unwrap(),
                "`{name}` payload must not be plaintext"
            );
        }
    })
    .unwrap()
}
