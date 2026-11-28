//! Each branch of `field_encryption_source`, the one encryption decision the
//! block builder and the KMS preparation share.

use datastore::{NamespaceView, SharedTxn};
use db::block::builder::*;
use defra_core::block::Block;
use defra_core::encryption::EncryptionConfig;
use document::{Document, NormalValue, WritePreparation};
use storage::corekv::Store;
use storage::namespace::Namespace;
use storage::RegolithStore;

struct Created {
    blockstore: NamespaceView,
    headstore: NamespaceView,
    doc: Document,
    snapshot: DocHeadsSnapshot,
    field_cids: Vec<cid::Cid>,
}

/// Create one document with `fields` set, under `config`, and load its heads.
async fn create(fields: &[&str], config: Option<&EncryptionConfig>) -> Created {
    let store = RegolithStore::in_memory().unwrap();
    let txn = store.new_txn(false).await.unwrap();
    let shared = SharedTxn::new(txn, storage::Crdts::default());
    let blockstore = NamespaceView::new(shared.clone(), Namespace::Blockstore);
    let headstore = NamespaceView::new(shared, Namespace::Headstore);
    let mut doc = Document::new(crate::common::schema::open_collection(&[
        "name", "age", "bio",
    ]));
    for field in fields {
        doc.set(*field, NormalValue::String(format!("{field} value")))
            .unwrap();
    }
    let identity = DocRef::new(1, 1);
    let created = write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        &crate::common::schema::version("schema-v1"),
        identity,
        None,
        config,
        None,
        None,
    )
    .await
    .unwrap();
    let snapshot = DocHeadsSnapshot::load(&headstore, identity.doc_short_id)
        .await
        .unwrap();
    doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
    Created {
        blockstore,
        headstore,
        doc,
        snapshot,
        field_cids: created.field_cids,
    }
}

fn encrypt_fields(fields: &[&str]) -> EncryptionConfig {
    EncryptionConfig {
        encrypt_doc: false,
        encrypt_fields: fields.iter().map(|field| field.to_string()).collect(),
    }
}

fn encrypt_doc() -> EncryptionConfig {
    EncryptionConfig {
        encrypt_doc: true,
        encrypt_fields: vec![],
    }
}

async fn source(
    created: &Created,
    field: &str,
    config: Option<&EncryptionConfig>,
) -> FieldEncryptionSource {
    let heads = created.snapshot.field_heads(field);
    let mut scope = DocScope::new(Some(&created.snapshot), None);
    field_encryption_source(&created.blockstore, &heads, field, config, &mut scope)
        .await
        .unwrap()
}

async fn encryption_link(created: &Created, cid: &cid::Cid) -> Option<cid::Cid> {
    let bytes = created
        .blockstore
        .get(&cid.to_bytes())
        .await
        .unwrap()
        .unwrap();
    Block::from_dag_cbor(&bytes).unwrap().encryption
}

/// (a) An explicit config wins over plaintext heads, scoped to the field when
/// it lists the field and to the document when it encrypts the whole.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn explicit_config_wins_over_plaintext_heads() {
    workers::block_on(async {
        let created = create(&["name"], None).await;
        assert_eq!(
            source(&created, "name", Some(&encrypt_fields(&["name"]))).await,
            FieldEncryptionSource::Explicit { individual: true }
        );
        assert_eq!(
            source(&created, "name", Some(&encrypt_doc())).await,
            FieldEncryptionSource::Explicit { individual: false }
        );
    })
    .unwrap()
}

/// (b) A field with encrypted heads reuses its own link.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn encrypted_heads_are_inherited() {
    workers::block_on(async {
        let created = create(&["name"], Some(&encrypt_doc())).await;
        let link = encryption_link(&created, &created.field_cids[0])
            .await
            .unwrap();
        assert_eq!(
            source(&created, "name", None).await,
            FieldEncryptionSource::Inherit(link)
        );
    })
    .unwrap()
}

/// (b) A field with plaintext heads stays plaintext without config, even when
/// every other field is encrypted.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn plaintext_heads_stay_plaintext_beside_encrypted_fields() {
    workers::block_on(async {
        let created = create(&["name", "age"], Some(&encrypt_fields(&["age"]))).await;
        assert_eq!(
            source(&created, "name", None).await,
            FieldEncryptionSource::Plaintext
        );
    })
    .unwrap()
}

/// (b) Mixed heads, one plaintext and one encrypted, follow the first
/// encrypted one, as Go's `determineBlockEncryption` skips plaintext heads.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn mixed_heads_inherit_the_first_encrypted_head() {
    workers::block_on(async {
        let mut created = create(&["name"], None).await;
        let plain_cid = created.field_cids[0];
        created
            .doc
            .set("name", NormalValue::String("new".to_string()))
            .unwrap();
        let updated = write_document_blocks(
            &created.blockstore,
            &created.headstore,
            &created.doc,
            &crate::common::schema::version("schema-v1"),
            DocRef::new(1, 1),
            Some(&["name".to_string()].into_iter().collect()),
            Some(&encrypt_fields(&["name"])),
            None,
            None,
        )
        .await
        .unwrap();
        let enc_cid = updated.field_cids[0];
        let link = encryption_link(&created, &enc_cid).await.unwrap();
        let heads = [plain_cid, enc_cid];
        let mut scope = DocScope::new(Some(&created.snapshot), None);
        let source = field_encryption_source(&created.blockstore, &heads, "name", None, &mut scope)
            .await
            .unwrap();
        assert_eq!(source, FieldEncryptionSource::Inherit(link));
    })
    .unwrap()
}

/// (c) A field with no heads on a document whose every field is encrypted gets
/// a fresh document-scope key.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn headless_field_of_an_all_encrypted_doc_gets_a_doc_key() {
    workers::block_on(async {
        let created = create(&["name", "age"], Some(&encrypt_doc())).await;
        assert_eq!(
            source(&created, "bio", None).await,
            FieldEncryptionSource::DocScopeNew
        );
    })
    .unwrap()
}

/// (c) One plaintext field is enough for a field with no heads to stay
/// plaintext.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn headless_field_of_a_partly_encrypted_doc_stays_plaintext() {
    workers::block_on(async {
        let created = create(&["name", "age"], Some(&encrypt_fields(&["age"]))).await;
        assert_eq!(
            source(&created, "bio", None).await,
            FieldEncryptionSource::Plaintext
        );
    })
    .unwrap()
}

/// (c) A document with no fields is not "every field encrypted": the empty
/// case is plaintext, never vacuously encrypted.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn headless_field_of_an_empty_doc_stays_plaintext() {
    workers::block_on(async {
        let created = create(&[], None).await;
        assert_eq!(created.snapshot.fields().count(), 0);
        assert_eq!(
            source(&created, "bio", None).await,
            FieldEncryptionSource::Plaintext
        );
    })
    .unwrap()
}

/// A KMS preparation's decision for a field with no heads wins over the
/// builder's snapshot, so the builder never needs a key prepare did not mint.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn kms_preparation_decides_headless_fields() {
    workers::block_on(async {
        let created = create(&["name"], None).await;
        let mut prepared = WritePreparation::new(1, 1);
        prepared.set_requires_kms_keys(true);
        prepared.doc_scope_fields.insert("bio".to_string());
        let heads = created.snapshot.field_heads("bio");
        let mut scope = DocScope::new(Some(&created.snapshot), Some(&prepared));
        let bio = field_encryption_source(&created.blockstore, &heads, "bio", None, &mut scope)
            .await
            .unwrap();
        assert_eq!(bio, FieldEncryptionSource::DocScopeNew);
        let age = field_encryption_source(&created.blockstore, &heads, "age", None, &mut scope)
            .await
            .unwrap();
        assert_eq!(age, FieldEncryptionSource::Plaintext);
    })
    .unwrap()
}
