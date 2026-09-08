use std::sync::Arc;

use acp::{DocumentACP as _, DocumentPermission, Identity};
use commonware_codec::Encode as _;
use integration_test::USER_ACP_POLICY;
use sourcehub::{AcpTuning, HubRsProvider, SourceHubDocumentACP, SourceHubProvider};

use super::helpers;

#[tokio::test]
#[serial_test::serial]
async fn archived_owner_record_does_not_authorize_access() {
    let hub = helpers::start_hub_cluster().await;
    let keys = hub_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .expect("bootstrap keys");
    let consensus_key = hex::encode(keys.epoch_info().output.public().public().encode());
    let worker_dir = tempfile::tempdir().unwrap();
    let keyring =
        keyring::FileKeyring::open(worker_dir.path().join("keys"), b"test-password").unwrap();
    let worker =
        sourcehub::hub_rs::NativeWorker::open(&worker_dir.path().join("worker"), &keyring, 9001)
            .unwrap();
    let owner = helpers::funded_identity();
    let private_key = hex::decode(&owner.private_key_hex).expect("owner key");
    let provider = Arc::new(
        HubRsProvider::new(
            hub.node(0).rpc_url(),
            &consensus_key,
            &private_key,
            worker,
            &AcpTuning::default(),
            None,
        )
        .await
        .expect("provider"),
    );
    let owner_did = provider.self_did().unwrap();
    let policy = provider
        .create_policy(USER_ACP_POLICY)
        .await
        .expect("policy");
    let token = provider
        .create_bearer_token(&owner_did)
        .await
        .expect("delegation");
    provider
        .register_object(&token, &policy, "users", "archived-document")
        .await
        .expect("register object");
    assert!(provider
        .verify_access(&policy, "users", "archived-document", "owner", &owner_did)
        .await
        .expect("owner proof"));
    let document_acp = SourceHubDocumentACP::without_access_cache(provider.clone());
    assert_eq!(
        document_acp
            .get_doc_owner(&policy, "users", "archived-document")
            .await
            .unwrap()
            .unwrap()
            .as_str(),
        owner_did
    );
    assert!(document_acp
        .is_doc_registered(&policy, "users", "archived-document")
        .await
        .unwrap());
    assert_eq!(
        provider
            .query_object_owner(&format!("0x{policy}"), "users", "archived-document")
            .await
            .unwrap(),
        (true, owner_did.clone())
    );
    assert!(!document_acp
        .is_doc_registered(&policy, "users", "never-registered")
        .await
        .unwrap());
    assert!(provider
        .query_object_owner(&policy, "users/other", "archived-document")
        .await
        .is_err());
    document_acp
        .unregister_doc_object(&policy, "users", "archived-document")
        .await
        .expect("archive through verified owner lookup");
    assert!(!document_acp
        .is_doc_registered(&policy, "users", "archived-document")
        .await
        .unwrap());
    assert!(document_acp
        .get_doc_owner(&policy, "users", "archived-document")
        .await
        .unwrap()
        .is_none());
    for _ in 0..2 {
        assert!(
            !provider
                .verify_access(&policy, "users", "archived-document", "owner", &owner_did)
                .await
                .expect("archived owner proof"),
            "retained ownership must not grant access after archival"
        );
    }
    for identity in [
        Identity::anonymous(),
        Identity::authenticated(identity::Did::new(&owner_did).unwrap()),
    ] {
        for object_id in ["archived-document", "never-registered"] {
            assert!(
                !document_acp
                    .check_doc_access(
                        &identity,
                        DocumentPermission::Read,
                        &policy,
                        "users",
                        object_id
                    )
                    .await
                    .expect("document access"),
                "missing registration must not make a Vera policy document public"
            );
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn native_permissions_honor_policy_exclusions_and_cross_object_rules() {
    use sourcehub::SubjectRef;
    let mut hub = helpers::start_hub_cluster().await;
    let keys = hub_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .unwrap();
    let consensus_key = hex::encode(keys.epoch_info().output.public().public().encode());
    let worker_dir = tempfile::tempdir().unwrap();
    let keyring =
        keyring::FileKeyring::open(worker_dir.path().join("keys"), b"test-password").unwrap();
    let worker =
        sourcehub::hub_rs::NativeWorker::open(&worker_dir.path().join("worker"), &keyring, 9001)
            .unwrap();
    let owner = helpers::funded_identity();
    let provider = Arc::new(
        HubRsProvider::new(
            hub.node(0).rpc_url(),
            &consensus_key,
            &hex::decode(&owner.private_key_hex).unwrap(),
            worker,
            &AcpTuning::default(),
            None,
        )
        .await
        .unwrap(),
    );
    let owner_did = provider.self_did().unwrap();
    let policy = provider
        .create_policy(
            r#"name: permissions
resources:
  - name: file
    relations:
      - name: reader
      - name: blocked
      - name: approved
    permissions:
      - name: read
        expr: (reader & approved) - blocked->blocked
  - name: group
    relations:
      - name: member
        types: [actor]
"#,
        )
        .await
        .unwrap();
    let bearer = provider.create_bearer_token(&owner_did).await.unwrap();
    provider
        .register_object(&bearer, &policy, "file", "report")
        .await
        .unwrap();
    let reader = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
    let subject = SubjectRef::Actor(reader.into());
    provider
        .set_relationship(&bearer, &policy, "file", "report", "reader", &subject)
        .await
        .unwrap();
    assert!(
        !provider
            .verify_access(&policy, "file", "report", "read", reader)
            .await
            .unwrap(),
        "a reader relation alone does not satisfy the policy"
    );
    provider
        .register_object(&bearer, &policy, "group", "editors")
        .await
        .unwrap();
    provider
        .set_relationship(&bearer, &policy, "group", "editors", "member", &subject)
        .await
        .unwrap();
    provider
        .set_relationship_subject(
            &policy, "file", "report", "approved", 3, "group", "editors", "member",
        )
        .await
        .unwrap();
    assert!(provider
        .verify_access(&policy, "file", "report", "read", reader)
        .await
        .unwrap());
    let first_decision = provider
        .create_access_decision(&policy, "file", "report", "read", reader)
        .await
        .unwrap()
        .unwrap();
    let second_decision = provider
        .create_access_decision(&policy, "file", "report", "read", reader)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        first_decision, second_decision,
        "each signed submission must have its own decision"
    );
    provider
        .set_relationship_subject(
            &policy,
            "file",
            "report",
            "blocked",
            3,
            "file",
            "suspensions",
            "blocked",
        )
        .await
        .unwrap();
    provider
        .set_relationship(&bearer, &policy, "file", "suspensions", "blocked", &subject)
        .await
        .unwrap();
    assert!(
        !provider
            .verify_access(&policy, "file", "report", "read", reader)
            .await
            .unwrap(),
        "cross-object exclusions must override the reader grant"
    );
    assert!(
        provider
            .create_access_decision(&policy, "file", "report", "read", reader)
            .await
            .is_err(),
        "a revoked actor must not obtain a decision"
    );
    assert!(provider
        .verify_access(&policy, "file", "report", "read", &owner_did)
        .await
        .unwrap());
    let document_acp = SourceHubDocumentACP::without_access_cache(provider.clone());
    let identity = Identity::authenticated(identity::Did::new(reader).unwrap());
    assert!(!document_acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            &policy,
            "file",
            "report"
        )
        .await
        .unwrap());
    provider
        .delete_relationship(&bearer, &policy, "file", "suspensions", "blocked", &subject)
        .await
        .unwrap();
    assert!(document_acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            &policy,
            "file",
            "report"
        )
        .await
        .unwrap());
    provider
        .delete_relationship(&bearer, &policy, "group", "editors", "member", &subject)
        .await
        .unwrap();
    assert!(!document_acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            &policy,
            "file",
            "report"
        )
        .await
        .unwrap());
    assert!(provider
        .create_access_decision(&policy, "file", "report", "read", reader)
        .await
        .is_err());
    provider
        .set_relationship(&bearer, &policy, "group", "editors", "member", &subject)
        .await
        .unwrap();
    let spec_policy = provider
        .create_policy(
            "spec: defra\nname: files\nresources:\n  - name: file\n    relations:\n      - name: writer\n    permissions:\n      - name: read\n      - name: write\n        expr: writer\n",
        )
        .await
        .unwrap();
    provider
        .register_object(&bearer, &spec_policy, "file", "report")
        .await
        .unwrap();
    provider
        .set_relationship(&bearer, &spec_policy, "file", "report", "writer", &subject)
        .await
        .unwrap();
    assert!(document_acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            &spec_policy,
            "file",
            "report"
        )
        .await
        .unwrap());
    hub.kill_node(0);
    // Outlive the native client's default maximum revision age.
    tokio::time::sleep(std::time::Duration::from_secs(31)).await;
    let error = document_acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            &policy,
            "file",
            "report",
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("stale"), "{error:?}");
    let error = provider
        .query_object_owner(&policy, "file", "report")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("stale"), "{error:?}");

    hub.restart_node(0).unwrap();
    hub.wait_ready(std::time::Duration::from_secs(30))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match document_acp
                .check_doc_access(
                    &identity,
                    DocumentPermission::Read,
                    &policy,
                    "file",
                    "report",
                )
                .await
            {
                Ok(allowed) => {
                    assert!(allowed);
                    break;
                }
                Err(error) => {
                    assert!(error.to_string().contains("stale"), "{error:?}");
                    let error = provider
                        .query_object_owner(&policy, "file", "report")
                        .await
                        .unwrap_err();
                    assert!(error.to_string().contains("stale"), "{error:?}");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    })
    .await
    .expect("fresh verified revision after Hub restart");
    assert!(document_acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            &spec_policy,
            "file",
            "report"
        )
        .await
        .unwrap());
    provider
        .delete_relationship(&bearer, &spec_policy, "file", "report", "writer", &subject)
        .await
        .unwrap();
    assert!(!document_acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            &spec_policy,
            "file",
            "report"
        )
        .await
        .unwrap());

    assert_eq!(
        provider
            .query_object_owner(&policy, "file", "report")
            .await
            .unwrap(),
        (true, owner_did)
    );
}
