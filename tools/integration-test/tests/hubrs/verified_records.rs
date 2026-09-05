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
