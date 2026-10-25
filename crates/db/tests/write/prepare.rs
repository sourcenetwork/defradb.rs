//! Preparing a write before its transaction: a signed create on a node with no
//! KMS gets its preparation here, with its signed blocks attached to it.

use crate::common::{fixture::make_test_db_with_bus, schema::test_collection};
use crypto::{Key, PrivateKey};
use db::AutoCommitMutator;
use defra_core::signing::{set_signing_config, SigningConfig, SigningKeyType};
use defra_core::Block;
use document::Document;
use query::mutator::DocMutator;

#[test]
fn a_signed_create_without_a_kms_commits_the_blocks_prepared_for_it() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db);
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
        let prepared = mutator
            .prepare_write(
                "TestDoc",
                Document::from_json_str(r#"{"x":1}"#).unwrap(),
                None,
                None,
            )
            .await;
        set_signing_config(None);
        let prepared = prepared.unwrap();
        assert!(prepared.write_preparation().is_some());

        // Signing is off now, so a signed commit block can only come from the
        // blocks prepared above.
        let created = mutator.create("TestDoc", prepared).await.unwrap();
        let block = Block::from_dag_cbor(created.commit_block.as_ref().unwrap()).unwrap();
        assert!(block.signature.is_some(), "{block:?}");
    })
    .unwrap()
}
