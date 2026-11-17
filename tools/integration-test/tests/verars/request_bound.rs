//! A delegation bound to one request, as the Vera node itself takes it.
//!
//! DefraDB checks the binding before it sends anything. Here the provider is
//! handed the token directly, behind that check, so the answer is the
//! chain's own: it executes exactly the command the token names, for the
//! deployment it names, and a second submission of it changes nothing.

use std::time::Duration;

use vera::vera_rs::{
    caller_delegation, request_delegation, request_digest, DelegationSigner, Ed25519Signer,
    RequestBinding,
};
use vera::{PolicyAction, PolicyCommand, PolicySubject, ProviderError, SubjectRef, VeraProvider};

use super::embedded::{self, Embedded};

const READER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
const LIFETIME: Duration = Duration::from_secs(120);
const POLICY: &str = "\
name: request bound
resources:
  - name: users
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
    relations:
      - name: reader
        types:
          - actor
";

/// The deployment's genesis identity: the parent of its first block.
async fn genesis(rpc_url: &str) -> [u8; 32] {
    let answer: serde_json::Value = net::http::Client::builder(edge::testing::edge())
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("client")
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "vera_getLightBlock", "params": ["0x1"]
        }))
        .send()
        .await
        .expect("answer")
        .json()
        .await
        .expect("JSON");
    let parent = answer["result"]["parent_hash"]
        .as_str()
        .unwrap_or_else(|| panic!("no first block: {answer}"));
    hex::decode(parent.trim_start_matches("0x"))
        .expect("hexadecimal")
        .try_into()
        .expect("32 bytes")
}

#[nuclei::test]
#[serial_test::serial]
async fn the_chain_executes_a_request_bound_delegation_for_its_command_only() {
    edge::compat(async {
        let node = Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        let provider = node
            .in_process(embedded::worker(root.path()))
            .await
            .expect("provider");
        let policy = provider.create_policy(POLICY).await.expect("policy");
        let audience = provider.delegation_audience().expect("audience");
        let genesis = genesis(&node.rpc_url()).await;
        let user = Ed25519Signer::from_slice(&[9; 32]).expect("key");
        let reader = SubjectRef::Actor(READER.to_string());
        let on = |object_id, action| PolicyCommand {
            policy_id: &policy,
            resource: "users",
            object_id,
            action,
        };
        let bound = |command: PolicyCommand<'_>, genesis_id: [u8; 32], nonce: u8| {
            let digest = request_digest(&command).expect("digest");
            let (user, audience) = (&user, &audience);
            async move {
                request_delegation(
                    user,
                    audience,
                    &RequestBinding {
                        digest,
                        genesis_id,
                        nonce: [nonce; 24],
                    },
                    LIFETIME,
                )
                .await
                .expect("delegation")
            }
        };
        let owner = |object: &'static str| {
            let (provider, policy) = (&provider, &policy);
            async move {
                provider
                    .query_object_owner(policy, "users", object)
                    .await
                    .expect("owner")
            }
        };
        let reads = || async {
            provider
                .verify_access(&policy, "users", "first", "read", READER)
                .await
                .expect("decision")
        };

        // Bound to the registration of one document and sent with another.
        let registering = bound(on("first", PolicyAction::RegisterObject), genesis, 1).await;
        assert!(matches!(
            provider
                .register_object(&registering, &policy, "users", "second")
                .await,
            Err(ProviderError::Transaction(_))
        ));
        assert!(!owner("second").await.0);
        // The node's own check says the same before anything is sent.
        assert!(matches!(
            provider.caller_of_request_delegation(
                &registering,
                &on("second", PolicyAction::RegisterObject)
            ),
            Some(Err(_))
        ));
        assert_eq!(
            provider
                .caller_of_request_delegation(
                    &registering,
                    &on("first", PolicyAction::RegisterObject)
                )
                .expect("bound to a request")
                .expect("its request"),
            user.did()
        );

        // Bound to the right command on another deployment's genesis: only
        // the chain knows its genesis identity, and it refuses.
        let elsewhere = bound(on("first", PolicyAction::RegisterObject), [7; 32], 2).await;
        assert!(matches!(
            provider
                .register_object(&elsewhere, &policy, "users", "first")
                .await,
            Err(ProviderError::Transaction(_))
        ));
        assert!(!owner("first").await.0);

        provider
            .register_object(&registering, &policy, "users", "first")
            .await
            .expect("registration");
        let (registered, registrant) = owner("first").await;
        assert!(registered);
        assert_eq!(registrant, user.did());

        // A grant under its own delegation, then revoked under a plain one.
        let grant = PolicyAction::SetRelationship {
            relation: "reader",
            subject: PolicySubject::Actor(&reader),
        };
        let granting = bound(on("first", grant), genesis, 3).await;
        // The registration's delegation does not grant.
        assert!(matches!(
            provider
                .set_relationship(&registering, &policy, "users", "first", "reader", &reader)
                .await,
            Err(ProviderError::Transaction(_))
        ));
        assert!(!reads().await);
        provider
            .set_relationship(&granting, &policy, "users", "first", "reader", &reader)
            .await
            .expect("grant");
        assert!(reads().await);
        let plain = caller_delegation(&user, &audience, LIFETIME)
            .await
            .expect("delegation");
        provider
            .delete_relationship(&plain, &policy, "users", "first", "reader", &reader)
            .await
            .expect("revocation");
        assert!(!reads().await);

        // Sent again, the grant's delegation is answered with what it did
        // the first time and does nothing: the revocation stands.
        provider
            .set_relationship(&granting, &policy, "users", "first", "reader", &reader)
            .await
            .expect("the recorded outcome");
        assert!(!reads().await);

        node.stop().await;
    })
    .await;
}
