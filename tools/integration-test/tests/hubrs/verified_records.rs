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
    let owner = helpers::funded_identity();
    let private_key = hex::decode(&owner.private_key_hex).expect("owner key");
    let provider = Arc::new(
        HubRsProvider::new(
            hub.node(0).rpc_url(),
            &consensus_key,
            &private_key,
            &AcpTuning::default(),
            None,
        )
        .await
        .expect("provider"),
    );
    let owner_did = provider.authorized_account();
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
    provider
        .archive_object(&token, &policy, "users", "archived-document")
        .await
        .expect("archive object");
    for _ in 0..2 {
        assert!(
            !provider
                .verify_access(&policy, "users", "archived-document", "owner", &owner_did)
                .await
                .expect("archived owner proof"),
            "retained ownership must not grant access after archival"
        );
    }
    let document_acp = SourceHubDocumentACP::without_access_cache(provider);
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
    let owner = helpers::funded_identity();
    let provider = Arc::new(
        HubRsProvider::new(
            hub.node(0).rpc_url(),
            &consensus_key,
            &hex::decode(&owner.private_key_hex).unwrap(),
            &AcpTuning::default(),
            None,
        )
        .await
        .unwrap(),
    );
    let owner_did = provider.authorized_account();
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
        .set_relationship(&bearer, &policy, "file", "report", "approved", &subject)
        .await
        .unwrap();
    assert!(provider
        .verify_access(&policy, "file", "report", "read", reader)
        .await
        .unwrap());
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
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    })
    .await
    .expect("fresh verified revision after Hub restart");
}
