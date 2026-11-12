use std::sync::Arc;

use commonware_codec::Encode as _;
use integration_test::USER_ACP_POLICY;
use vera::{AcpTuning, SubjectRef, VeraProvider, VeraRsProvider};

use super::helpers;

/// DefraDB's spelling of a fresh secp256k1 actor, and vera.rs's.
fn reader_spellings() -> (String, String) {
    use identity::Identity as _;
    let reader = identity::RawIdentity::from_secp256k1(crypto::generate_secp256k1().unwrap())
        .expect("reader identity");
    let defra = reader.did().expect("reader did").to_string();
    let key = vera_client::Secp256k1SigningKey::from_slice(&reader.private_key_bytes())
        .expect("reader key");
    let compressed = key.verifying_key().to_sec1_point(true);
    let vera = crypto::create_did_key(crypto::KeyType::Secp256k1, compressed.as_bytes())
        .expect("compressed did");
    (defra, vera)
}

#[nuclei::test]
#[serial_test::serial]
async fn either_spelling_of_a_secp256k1_actor_is_one_vera_actor() {
    edge::compat(async {
        let hub = helpers::start_hub_cluster().await;
        let keys = vera_harness::cluster::KeySet::builder()
            .nodes(1)
            .seed(0)
            .build()
            .expect("bootstrap keys");
        let consensus_key = hex::encode(keys.epoch_info().output.public().public().encode());
        let worker_dir = tempfile::tempdir().unwrap();
        let keyring =
            keyring::FileKeyring::open(worker_dir.path().join("keys"), b"test-password").unwrap();
        let worker =
            vera::vera_rs::NativeWorker::open(&worker_dir.path().join("worker"), &keyring, 9001)
                .unwrap();
        let owner = helpers::funded_identity();
        let private_key = hex::decode(&owner.private_key_hex).expect("owner key");
        let provider = Arc::new(
            VeraRsProvider::new(
                hub.node(0).rpc_url(),
                &consensus_key,
                &private_key,
                worker,
                &AcpTuning::default(),
                None,
                &edge::testing::edge(),
            )
            .await
            .expect("provider"),
        );

        // The provider's actor is the node's key, under DefraDB's spelling.
        assert_eq!(provider.self_did().as_deref(), Some(owner.did.as_str()));
        let policy = provider
            .create_policy(USER_ACP_POLICY)
            .await
            .expect("policy");
        let token = provider
            .create_bearer_token(&owner.did)
            .await
            .expect("delegation");
        provider
            .register_object(&token, &policy, "users", "document")
            .await
            .expect("register object");
        assert_eq!(
            provider
                .query_object_owner(&policy, "users", "document")
                .await
                .unwrap(),
            (true, owner.did.clone())
        );

        let (granted_as_defra, checked_as_vera) = reader_spellings();
        let (checked_as_defra, granted_as_vera) = reader_spellings();
        assert_ne!(granted_as_defra, checked_as_vera);
        for (grant, check) in [
            (&granted_as_defra, &checked_as_vera),
            (&granted_as_vera, &checked_as_defra),
        ] {
            assert!(!provider
                .verify_access(&policy, "users", "document", "read", check)
                .await
                .expect("access before the grant"));
            provider
                .set_relationship(
                    &token,
                    &policy,
                    "users",
                    "document",
                    "reader",
                    &SubjectRef::Actor(grant.clone()),
                )
                .await
                .expect("grant");
            for did in [grant, check] {
                assert!(
                    provider
                        .verify_access(&policy, "users", "document", "read", did)
                        .await
                        .expect("access after the grant"),
                    "a grant to one spelling must authorize the other"
                );
            }
            provider
                .delete_relationship(
                    &token,
                    &policy,
                    "users",
                    "document",
                    "reader",
                    &SubjectRef::Actor(check.clone()),
                )
                .await
                .expect("revoke under the other spelling");
            assert!(!provider
                .verify_access(&policy, "users", "document", "read", grant)
                .await
                .expect("access after the revocation"));
        }
    })
    .await;
}
