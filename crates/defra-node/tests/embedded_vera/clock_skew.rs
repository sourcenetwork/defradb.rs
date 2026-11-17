//! A signer whose clock is off from the provider's. A delegation bound to one
//! request holds from the clock skew allowance before it was issued, so a
//! clock a little off is taken. One that does not hold at the provider's time
//! is refused before anything is written, with that time, and a delegation
//! issued at it is taken: a client recovers without asking its user.

use std::time::Duration;

use acp::RemotePolicies as _;
use defra_node::{RelationshipChange, SignerCredential};
use query::executor::DELEGATION_OUTSIDE_WINDOW_ERROR_CODE;
use vera::vera_rs::{request_delegation_at, RequestBinding, MAX_REQUEST_LIFETIME};
use vera::{PolicyAction, PolicyCommand, PolicySubject, SubjectRef};

use super::verified::{Attempt, Deployed, User, LIFETIME};
use super::{now, STRANGER};

const MINUTE: i64 = 60;

/// `user`'s delegation bound to `action` on `doc`, issued at `issued_at`.
async fn bound_at(
    on: &Deployed,
    user: &User,
    doc: &str,
    action: PolicyAction<'_>,
    nonce: u8,
    issued_at: u64,
    lifetime: Duration,
) -> String {
    let command = PolicyCommand {
        policy_id: &on.policy,
        resource: "users",
        object_id: doc,
        action,
    };
    let binding = RequestBinding::for_command(&on.audience, &command, [nonce; 24])
        .expect("the node reports its genesis identity");
    request_delegation_at(
        &user.delegations,
        &on.audience,
        &binding,
        lifetime,
        issued_at,
    )
    .await
    .expect("delegation")
}

/// The time of a signer whose clock is `offset` seconds off.
fn clock(offset: i64) -> u64 {
    now().saturating_add_signed(offset)
}

/// `user`'s verified create of `name`, under a registration delegation
/// issued at `issued_at`.
async fn create_at(
    on: &Deployed,
    user: &User,
    name: &str,
    nonce: u8,
    issued_at: u64,
    lifetime: Duration,
) -> Attempt {
    on.create(user, name, |doc| async move {
        SignerCredential::RequestBound(
            bound_at(
                on,
                user,
                &doc,
                PolicyAction::RegisterObject,
                nonce,
                issued_at,
                lifetime,
            )
            .await,
        )
    })
    .await
}

/// The attempt was taken, and the chain names `user` the owner once it commits.
async fn taken(on: &Deployed, user: &User, attempt: Attempt) {
    let Attempt { doc, txn, response } = attempt;
    assert!(!response.has_errors(), "{:?}", response.errors);
    on.node.commit_transaction(&txn).await.expect("commit");
    assert_eq!(
        on.owner_on_chain(&doc).await.as_deref(),
        Some(user.did.as_str())
    );
}

/// The attempt was refused for its delegation's window, nothing reached the
/// chain, and the refusal names the provider's time and says `says`, and
/// nothing about the signer's clock unless `says` does.
async fn refused_for_its_window(on: &Deployed, attempt: Attempt, says: &str) -> u64 {
    let Attempt { doc, txn, response } = attempt;
    let error = response.errors.first().expect("refused");
    assert_eq!(
        error.code(),
        Some(DELEGATION_OUTSIDE_WINDOW_ERROR_CODE),
        "{:?}",
        response.errors
    );
    assert!(error.message.contains(says), "{}", error.message);
    assert_eq!(
        error.message.contains("clock"),
        says.contains("clock"),
        "{}",
        error.message
    );
    let provider_time = error.provider_time().expect("the provider's time");
    assert!(
        provider_time.abs_diff(now()) <= 5,
        "{provider_time} is the node's time"
    );
    assert!(on.node.commit_transaction(&txn).await.is_err());
    assert_eq!(on.owner_on_chain(&doc).await, None);
    provider_time
}

/// A clock five seconds ahead or behind is taken, and so is one nine minutes
/// behind under a delegation that lasts longer than that.
#[test]
fn a_clock_a_little_off_is_taken() {
    workers::block_on(async {
        let on = Deployed::start(9421).await;
        let user = User::new();
        for (index, offset) in [5, -5].into_iter().enumerate() {
            let name = format!("off by {offset}");
            let attempt = create_at(&on, &user, &name, index as u8 + 1, clock(offset), LIFETIME);
            taken(&on, &user, attempt.await).await;
        }
        let behind = create_at(
            &on,
            &user,
            "nine minutes behind",
            3,
            clock(-9 * MINUTE),
            MAX_REQUEST_LIFETIME,
        );
        taken(&on, &user, behind.await).await;
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A clock a minute ahead, nine minutes behind under a delegation of two
/// minutes, or eleven minutes behind under the longest delegation is refused
/// before anything is written, with the provider's time. Issued again at that
/// time, the same create is taken. Only the clock ahead is said to be off: a
/// delegation that has ended says nothing about its signer's clock, since one
/// held past its lifetime by a signer whose clock is right ends the same way.
#[test]
fn a_clock_far_off_is_refused_with_the_providers_time_and_taken_at_it() {
    workers::block_on(async {
        let on = Deployed::start(9422).await;
        let user = User::new();
        let ahead = "the signer's clock is at least";
        let ended = "it has expired";
        let cases = [
            (MINUTE, LIFETIME, ahead),
            (-9 * MINUTE, LIFETIME, ended),
            (-11 * MINUTE, MAX_REQUEST_LIFETIME, ended),
        ];
        for (index, (offset, lifetime, says)) in cases.into_iter().enumerate() {
            let name = format!("off by {offset}");
            let nonce = 2 * index as u8 + 1;
            let attempt = create_at(&on, &user, &name, nonce, clock(offset), lifetime);
            let provider_time = refused_for_its_window(&on, attempt.await, says).await;
            let again = create_at(&on, &user, &name, nonce + 1, provider_time, lifetime);
            taken(&on, &user, again.await).await;
        }

        // A right clock, its delegation held past its two seconds: the same
        // refusal as the clock eleven minutes behind.
        let attempt = on
            .create(&user, "held", |doc| {
                let (on, user) = (&on, &user);
                async move {
                    let token = bound_at(
                        on,
                        user,
                        &doc,
                        PolicyAction::RegisterObject,
                        9,
                        clock(0),
                        Duration::from_secs(2),
                    )
                    .await;
                    edge::testing::edge().sleep(Duration::from_secs(4)).await;
                    SignerCredential::RequestBound(token)
                }
            })
            .await;
        refused_for_its_window(&on, attempt, ended).await;
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A signer that issues at the chain time the node reports is taken whatever
/// its own clock says; a grant under a delegation that does not hold is a
/// typed refusal that stages nothing.
#[test]
fn a_delegation_issued_at_the_reported_chain_time_is_taken() {
    workers::block_on(async {
        let on = Deployed::start(9423).await;
        let user = User::new();
        let vera = on.node.vera_acp().expect("vera.rs document ACP").clone();
        let mut chain_time = None;
        for _ in 0..100 {
            chain_time = vera.light_client_status().expect("status").chain_time;
            if chain_time.is_some() {
                break;
            }
            edge::testing::edge()
                .sleep(Duration::from_millis(100))
                .await;
        }
        let chain_time = chain_time.expect("the node reports the chain's time");
        assert!(chain_time.abs_diff(now()) <= 30, "{chain_time}");
        let Attempt { doc, txn, response } =
            create_at(&on, &user, "at chain time", 1, chain_time, LIFETIME).await;
        assert!(!response.has_errors(), "{:?}", response.errors);

        let reader = identity::Did::new(STRANGER).unwrap();
        let subject = SubjectRef::Actor(reader.to_string());
        let grant = || PolicyAction::SetRelationship {
            relation: "reader",
            subject: PolicySubject::Actor(&subject),
        };
        let stage = |token: String| {
            let (on, txn, doc, reader, user) = (&on, &txn, &doc, &reader, &user);
            async move {
                let context = defra_core::RequestContext::default()
                    .with_identity(Some(user.did.to_string()))
                    .with_request_delegation(user.did.to_string(), token);
                on.node
                    .add_dac_actor_relationship_change_for_in_txn(
                        txn,
                        &context,
                        "Users",
                        doc,
                        "reader",
                        reader.as_str(),
                    )
                    .await
            }
        };
        let early = bound_at(&on, &user, &doc, grant(), 2, clock(MINUTE), LIFETIME).await;
        match stage(early).await {
            Err(defra_node::Error::Acp(acp::Error::DelegationOutsideWindow { window, .. })) => {
                // Set back by the allowance, the start proves at least the
                // lead less the allowance.
                assert!(
                    matches!(
                        window.refusal,
                        acp::WindowRefusal::NotYetValid { ahead_by_at_least }
                            if (25..=30).contains(&ahead_by_at_least)
                    ),
                    "{window}"
                );
            }
            other => panic!("not refused for its window: {other:?}"),
        }
        let granting = bound_at(&on, &user, &doc, grant(), 3, chain_time, LIFETIME).await;
        assert!(matches!(
            stage(granting).await,
            Ok(RelationshipChange::Staged {
                authority_checked: true,
                ..
            })
        ));
        on.node.commit_transaction(&txn).await.expect("commit");
        assert_eq!(
            on.owner_on_chain(&doc).await.as_deref(),
            Some(user.did.as_str())
        );
        assert!(super::chain_reads(&on.node, &reader, &on.policy, &doc).await);
        drop(vera);
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
