//! The delegation audience a provider reports, and the genesis identity it
//! checks a request-bound delegation against.

use std::time::Duration;

use serde_json::Value;

use super::signer_tests::provider;
use super::transport_tests::Workers;
use super::VeraProvider as _;
use crate::vera_rs::client::tests::Recorded;
use crate::vera_rs::{request_delegation, DelegationSigner as _, Ed25519Signer, RequestBinding};
use crate::{PolicyAction, PolicyCommand};
use vera_delegation::testing::{Recording, Returns};

const GENESIS: [u8; 32] = [3; 32];

fn register() -> PolicyCommand<'static> {
    PolicyCommand {
        policy_id: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        resource: "users",
        object_id: "bae-first",
        action: PolicyAction::RegisterObject,
    }
}

#[nuclei::test]
async fn a_provider_checks_the_genesis_identity_it_reports() {
    let mut workers = Workers::new();
    let provider = provider(
        Recording::returning(Returns::Genuine),
        Recorded::answering(Ok(Value::Null)),
        &mut workers,
    )
    .await;
    let user = Ed25519Signer::from_slice(&[9; 32]).expect("key");
    let unknown = provider.delegation_audience().expect("audience");
    assert_eq!(unknown.genesis_id, None);
    assert!(matches!(
        RequestBinding::for_command(&unknown, &register(), [5; 24]),
        Err(crate::vera_rs::BearerError::NoGenesisIdentity)
    ));
    let elsewhere = RequestBinding {
        genesis_id: [7; 32],
        ..RequestBinding::for_command(
            &acp::DelegationAudience {
                genesis_id: Some(GENESIS),
                ..unknown.clone()
            },
            &register(),
            [5; 24],
        )
        .expect("binding")
    };
    let foreign = request_delegation(&user, &unknown, &elsewhere, Duration::from_secs(60))
        .await
        .expect("delegation");
    // Not knowing its genesis identity, the provider leaves it to the chain.
    assert_eq!(
        provider
            .caller_of_request_delegation(&foreign, &register())
            .expect("bound to a request")
            .expect("taken"),
        user.did()
    );

    provider.genesis_id.set(GENESIS).expect("unset");
    let known = provider.delegation_audience().expect("audience");
    assert_eq!(known.genesis_id, Some(GENESIS));
    assert_eq!(
        (&known.worker, known.deployment),
        (&unknown.worker, unknown.deployment)
    );
    let refused = provider
        .caller_of_request_delegation(&foreign, &register())
        .expect("bound to a request");
    assert!(
        matches!(&refused, Err(error) if error.to_string().contains("genesis")),
        "{refused:?}"
    );

    let binding = RequestBinding::for_command(&known, &register(), [6; 24]).expect("binding");
    let token = request_delegation(&user, &known, &binding, Duration::from_secs(60))
        .await
        .expect("delegation");
    assert_eq!(
        provider
            .caller_of_request_delegation(&token, &register())
            .expect("bound to a request")
            .expect("taken"),
        user.did()
    );
}
