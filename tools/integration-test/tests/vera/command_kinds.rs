//! Every policy command and subject kind the provider sends, taken by a
//! Vera node in the test process.
//!
//! The unit tests pin the bytes of each command to vera.rs's own command
//! type; this submits each one and checks the chain executes it.

use std::sync::Arc;

use vera::{SubjectRef, VeraProvider, VeraRsProvider};

use super::embedded::{self, Embedded};

const POLICY: &str = "\
name: command kinds
resources:
  - name: file
    relations:
      - name: reader
        types: [\"*\"]
    permissions:
      - name: read
        expr: reader
  - name: group
    relations:
      - name: member
        types: [actor]
";

const ED25519: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
/// The secp256k1 generator point, compressed and uncompressed: a key that is
/// neither the provider's actor nor any other actor of the test.
const SECP256K1_COMPRESSED: &str =
    "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const SECP256K1_UNCOMPRESSED: &str = "0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8";
/// The compressed and uncompressed spellings of one P-256 key.
const P256_COMPRESSED: &str = "did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv";
const P256_UNCOMPRESSED: &str = "did:key:z4oJ8cYF2JwS84CUKnKrnNW6hAhUzH3BNfybZEa87TkErqCeqTScZ4TFF565pwTYuoHbHbP6sR544QJf5tgQe13tFvfRt";
/// An actor no relationship names.
const STRANGER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

#[nuclei::test]
#[serial_test::serial]
async fn in_process_every_command_and_subject_kind_is_taken() {
    edge::compat(async {
        let node = Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        let provider = node
            .in_process(embedded::worker(root.path()))
            .await
            .expect("provider");
        every_command_and_subject_kind(Arc::new(provider)).await;
        node.stop().await;
    })
    .await;
}

async fn reads(provider: &VeraRsProvider, policy: &str, actor: &str) -> bool {
    provider
        .verify_access(policy, "file", "report", "read", actor)
        .await
        .expect("proven read")
}

/// Registers, relates under each subject kind, unrelates and archives,
/// checking each command is executed. Run against whatever node `provider`
/// reaches.
async fn every_command_and_subject_kind(provider: Arc<VeraRsProvider>) {
    let owner = provider.self_did().unwrap();
    let policy = provider.create_policy(POLICY, None).await.expect("policy");
    let bearer = provider.create_bearer_token(&owner).await.unwrap();
    for (resource, object) in [("file", "report"), ("group", "editors")] {
        provider
            .register_object(&bearer, &policy, resource, object, &[])
            .await
            .expect("register object");
    }

    // Each actor subject, in each spelling of its key, is granted and
    // revoked, and is read back under either spelling.
    let secp256k1 = |point: &str| {
        crypto::create_did_key(crypto::KeyType::Secp256k1, &hex::decode(point).unwrap()).unwrap()
    };
    let secp256k1 = [
        secp256k1(SECP256K1_COMPRESSED),
        secp256k1(SECP256K1_UNCOMPRESSED),
    ];
    let p256 = [P256_COMPRESSED, P256_UNCOMPRESSED].map(String::from);
    for (granted, spellings) in [
        (ED25519.to_string(), [ED25519, ED25519].map(String::from)),
        (secp256k1[0].clone(), secp256k1.clone()),
        (secp256k1[1].clone(), secp256k1.clone()),
        (p256[0].clone(), p256.clone()),
        (p256[1].clone(), p256.clone()),
    ] {
        let subject = SubjectRef::Actor(granted.clone());
        provider
            .set_relationship(&bearer, &policy, "file", "report", "reader", &subject)
            .await
            .unwrap_or_else(|e| panic!("grant to {granted}: {e}"));
        for actor in &spellings {
            assert!(
                reads(&provider, &policy, actor).await,
                "{granted} as {actor}"
            );
        }
        assert!(!reads(&provider, &policy, STRANGER).await);
        provider
            .delete_relationship(&bearer, &policy, "file", "report", "reader", &subject)
            .await
            .unwrap_or_else(|e| panic!("revocation from {granted}: {e}"));
        for actor in &spellings {
            assert!(
                !reads(&provider, &policy, actor).await,
                "{granted} as {actor}"
            );
        }
    }

    provider
        .set_relationship(
            &bearer,
            &policy,
            "file",
            "report",
            "reader",
            &SubjectRef::AllActors,
        )
        .await
        .expect("grant to every actor");
    assert!(reads(&provider, &policy, STRANGER).await);
    provider
        .delete_relationship(
            &bearer,
            &policy,
            "file",
            "report",
            "reader",
            &SubjectRef::AllActors,
        )
        .await
        .expect("revocation from every actor");
    assert!(!reads(&provider, &policy, STRANGER).await);

    provider
        .set_relationship(
            &bearer,
            &policy,
            "group",
            "editors",
            "member",
            &SubjectRef::Actor(ED25519.into()),
        )
        .await
        .expect("membership");
    // Each structured subject kind: an actor (in a spelling vera.rs is sent
    // another of), every actor, another object, and a userset.
    for (kind, resource, object, relation, reader) in [
        (0, "", P256_UNCOMPRESSED, "", P256_COMPRESSED),
        (1, "", "", "", STRANGER),
        (2, "group", "editors", "", ""),
        (3, "group", "editors", "member", ED25519),
    ] {
        provider
            .set_relationship_subject_as(
                &bearer, &policy, "file", "report", "reader", kind, resource, object, relation,
            )
            .await
            .unwrap_or_else(|e| panic!("subject kind {kind} grant: {e}"));
        if !reader.is_empty() {
            assert!(
                reads(&provider, &policy, reader).await,
                "subject kind {kind}"
            );
        }
        provider
            .delete_relationship_subject_as(
                &bearer, &policy, "file", "report", "reader", kind, resource, object, relation,
            )
            .await
            .unwrap_or_else(|e| panic!("subject kind {kind} revocation: {e}"));
        if !reader.is_empty() {
            assert!(
                !reads(&provider, &policy, reader).await,
                "subject kind {kind}"
            );
        }
    }

    let owns = || provider.verify_access(&policy, "file", "report", "owner", &owner);
    assert!(owns().await.expect("owner proof"));
    provider
        .archive_object(&bearer, &policy, "file", "report")
        .await
        .expect("archive");
    assert!(!owns().await.expect("archived owner proof"));
}
