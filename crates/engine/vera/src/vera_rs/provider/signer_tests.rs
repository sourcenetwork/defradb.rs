//! The provider's own delegations, signed by a signer the test controls.
//!
//! The provider here is assembled without a node: its light client follows a
//! transport that serves nothing, and its submissions go to a recording
//! transport, so each test sees what was signed and what would have been sent.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use acp_light_client::{AcpLightClient, FreshnessPolicy};
use alloy_primitives::Bytes;
use alloy_sol_types::SolCall as _;
use base64::Engine as _;
use serde_json::Value;
use vera_domain::NativeTx;

use super::transport_tests::{Silent, Workers};
use super::{
    trusted_key, Follow, Follower, IAcp, ProviderChain, ProviderError, VeraProvider, VeraRsProvider,
};
use crate::vera_rs::client::tests::{Recorded, TRUSTED_KEY};
use crate::vera_rs::client::VeraRsClient;
use crate::vera_rs::did_spelling;
use crate::vera_rs::signer::admission;
use crate::vera_rs::signer::DelegationSigner as _;
use vera_delegation::testing::{Recording, Returns};

pub(super) async fn provider(
    signer: Arc<Recording>,
    rpc: Arc<Recorded>,
    workers: &mut Workers,
) -> VeraRsProvider {
    let edge = edge::testing::edge();
    let light_client = edge::compat(async {
        AcpLightClient::with_transport(
            Arc::new(Silent::default()),
            TRUSTED_KEY,
            10,
            FreshnessPolicy::default(),
        )
        .expect("light client")
    })
    .await;
    let mut worker = workers.next();
    admission::actor_key(signer.as_ref()).expect("secp256k1 actor");
    let light_client = Arc::new(light_client);
    let client = Arc::new(VeraRsClient::with_transport(rpc));
    let follow = Follow::from_journal(worker.confirmed());
    let worker_did = worker.did().to_owned();
    let deployment = worker.deployment_id();
    let worker = Arc::new(regolith::sync::Mutex::new(worker));
    let follower = Arc::new(Follower {
        light_client: light_client.clone(),
        chain: ProviderChain {
            client: client.clone(),
            light_client: light_client.clone(),
            trusted: trusted_key(TRUSTED_KEY).expect("trusted key"),
            timeout: Duration::from_secs(5),
            fallback: Duration::from_millis(200),
        },
        worker: worker.clone(),
        follow,
        sink: regolith::sync::OnceCell::new(),
        shared: std::sync::atomic::AtomicBool::new(false),
        holds_key: Box::new(|_| false),
    });
    VeraRsProvider {
        light_client,
        client,
        follower,
        worker_did,
        deployment,
        genesis_id: Arc::new(regolith::sync::OnceCell::new()),
        worker,
        actor_did: did_spelling::defra(signer.did()).into_owned(),
        trusted: trusted_key(TRUSTED_KEY).expect("trusted key"),
        signer,
        sync_timeout: Duration::from_secs(5),
        receipt_fallback: Duration::from_millis(200),
        light_client_observability: Arc::new(AtomicU64::new(0)),
        _light_client_observer: edge.spawn_service(std::future::pending()),
        edge,
    }
}

/// The one request `signer` answered, as the token it makes and its claims.
fn signed(signer: &Recording) -> (String, Value) {
    let (input, signature) = signer.signed.pop().expect("one signing request");
    assert!(signer.signed.pop().is_none(), "more than one request");
    let input = String::from_utf8(input).expect("signing input");
    let decoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let claims = decoder
        .decode(input.split('.').nth(1).expect("payload"))
        .expect("payload base64");
    (
        format!("{input}.{}", decoder.encode(signature)),
        serde_json::from_slice(&claims).expect("claims"),
    )
}

/// The delegation inside the one submission `rpc` was asked to send.
fn submitted(rpc: &Recorded) -> String {
    let mut wire = None;
    while let Some((method, params, _)) = rpc.calls.pop() {
        if method == "vera_sendNativeTx" {
            assert!(wire.is_none(), "more than one submission");
            wire = Some(serde_json::from_value::<Bytes>(params[0].clone()).expect("wire"));
        }
    }
    let tx = NativeTx::decode_wire(&wire.expect("one submission")).expect("signed request");
    IAcp::bearerCreatePolicyCall::abi_decode(&tx.calldata)
        .expect("policy creation call")
        .bearerToken
}

#[nuclei::test]
async fn the_provider_signs_its_delegations_through_its_signer() {
    let signer = Recording::returning(Returns::Genuine);
    let rpc = Recorded::answering(Ok(Value::Null));
    let mut workers = Workers::new();
    let provider = provider(signer.clone(), rpc.clone(), &mut workers).await;
    let actor = provider.self_did().expect("actor DID");
    let worker = provider.authorized_account();

    let token = provider
        .create_bearer_token(&actor)
        .await
        .expect("delegation");
    let (expected, claims) = signed(&signer);
    assert_eq!(token, expected);
    assert_eq!(claims["iss"], actor);
    assert_eq!(claims["sub"], worker);
    assert_eq!(claims["aud"], "vera:9001");
    assert_eq!(claims["scope"], "acp:policy");
    assert!(rpc.calls.pop().is_none());

    // The node answers nothing, so creation stops at its submission, which
    // carries the token the signer signed.
    let stopped = provider.create_policy("name: test", None).await;
    assert!(matches!(stopped, Err(ProviderError::Unavailable(_))));
    let (expected, claims) = signed(&signer);
    assert_eq!(submitted(&rpc), expected);
    assert_eq!(claims["iss"], actor);
    assert_eq!(claims["sub"], worker);
    assert_eq!(claims["scope"], "acp:policy:create");
}

#[nuclei::test]
async fn a_signature_vera_rs_would_refuse_is_never_sent() {
    let mut workers = Workers::new();
    for (returns, reason) in [
        (Returns::Truncated, "returned a 63-byte signature"),
        (Returns::Padded, "returned a 65-byte signature"),
        (Returns::Zero, "r or s is not a scalar"),
        (Returns::HighS, "s in the upper half of the curve order"),
        (Returns::OtherKey, "does not verify"),
        (Returns::Failure, "signing failed: the key is locked"),
    ] {
        let signer = Recording::returning(returns);
        let rpc = Recorded::answering(Ok(Value::Null));
        let provider = provider(signer.clone(), rpc.clone(), &mut workers).await;
        let actor = provider.self_did().expect("actor DID");

        for refused in [
            provider.create_bearer_token(&actor).await.map(drop),
            provider.create_policy("name: test", None).await.map(drop),
        ] {
            assert!(
                matches!(&refused, Err(ProviderError::Config(said)) if said.contains(reason)),
                "{returns:?}: {refused:?}"
            );
        }
        assert!(rpc.calls.pop().is_none(), "{returns:?}");
        assert!(provider.worker.lock().await.pending().is_none());
    }
}

#[nuclei::test]
async fn the_drainer_sends_nothing_while_the_provider_is_not_admitted() {
    let signer = Recording::returning(Returns::Genuine);
    let rpc = Recorded::answering(Ok(Value::Null));
    let mut workers = Workers::new();
    let provider = provider(signer, rpc.clone(), &mut workers).await;
    {
        let mut worker = provider.worker.clone().lock_owned().await;
        worker
            .confirmed()
            .close("a term voided more than the journal keeps".into())
            .expect("closed");
        provider.follower.follow.update(worker.confirmed());
    }
    let floor = provider.owed_request_floor().await;
    let journaled = provider.journal_owed_request(b"request").await;
    let outcome = provider
        .owed_request_outcome("0xdigest", None, Some(0))
        .await;
    for refused in [floor.err(), journaled.err(), outcome.err()] {
        assert!(
            matches!(refused, Some(ProviderError::Unavailable(ref reason)) if reason.contains("voided more")),
            "{refused:?}"
        );
    }
    assert!(provider
        .worker
        .clone()
        .lock_owned()
        .await
        .pending()
        .is_none());
    assert!(rpc.calls.pop().is_none(), "nothing reached the node");
}
