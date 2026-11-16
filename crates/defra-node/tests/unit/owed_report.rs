//! What a node owes a remote document ACP provider after a commit, as its
//! callers act on it: the report, the wait for one transaction, a fresh
//! credential for one owed registration or change, and abandoning a refused
//! change.

use std::sync::atomic::Ordering;
use std::time::Duration;

use acp::{OwedOutcome, OwedRef, OwedState, OwedWait, OwedWhat};
use identity::Did;

use super::staged_relationship_tests::{actor, request_token, READER};
use super::verified_remote_tests::{Attempt, Verified, STRANGER};
use super::{RelationshipChange, SignerCredential};

const WAIT: Duration = Duration::from_secs(5);

fn unavailable<T>(answer: &crate::Result<T>) -> bool {
    matches!(
        answer,
        Err(crate::Error::Acp(
            acp::Error::OwedAuthorityUnavailable { .. }
        ))
    )
}

fn not_found<T>(answer: &crate::Result<T>) -> bool {
    matches!(
        answer,
        Err(crate::Error::Acp(acp::Error::OwedEntryNotFound { .. }))
    )
}

fn denied<T>(answer: &crate::Result<T>) -> bool {
    matches!(
        answer,
        Err(crate::Error::Query(query::QueryError::PermissionDenied(_)))
    )
}

impl Verified {
    /// The user as the caller of a node call.
    fn caller(&self) -> defra_core::RequestContext {
        actor(&self.user)
    }

    async fn report(&self) -> Vec<acp::OwedEntry> {
        self.node
            .owed_acp_as(&self.caller(), &self.user)
            .await
            .unwrap()
    }

    /// The user carrying its plain delegation.
    fn delegating(&self) -> defra_core::RequestContext {
        self.caller().with_bearer(
            Some(self.user.clone()),
            Some(format!("delegation:{}", self.user)),
        )
    }

    /// The names of the records `did` reads.
    async fn read_by(&self, did: &str) -> Vec<String> {
        let response = self
            .node
            .execute_request(
                query::QueryRequest::new("{ Record { name } }")
                    .with_identity(Some(Did::new(did).unwrap())),
            )
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        response.data.unwrap()["Record"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| record["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

#[test]
fn a_registration_that_fails_after_its_commit_is_reported_and_takes_a_fresh_credential() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let Attempt { doc, txn, response } = on.create("owed", |doc| on.registration(doc)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let granting = request_token(&on.user, &format!("set {doc} reader {READER}"));
        assert!(matches!(
            on.grant(&txn, &on.bound(granting.clone()), &doc).await,
            Ok(RelationshipChange::Staged { .. })
        ));

        // Both delegations run out before the commit sends anything. The
        // commit stands all the same.
        on.remote.lapsed.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();
        assert!(on.remote.owners.get(&doc).is_none());
        assert_eq!(on.records().await, ["owed"]);

        let owed = on.report().await;
        assert_eq!(owed.len(), 2, "{owed:?}");
        let registration = owed[0].reference();
        assert_eq!(owed[0].what, OwedWhat::Registration);
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        assert!(
            owed[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("expired")),
            "{owed:?}"
        );
        assert_eq!(owed[0].txn, txn.as_str().parse().ok());
        assert!(!owed[0].sendable);
        // The grant waits behind it, neither sent nor parked.
        assert!(matches!(owed[1].what, OwedWhat::Change { .. }));
        assert_eq!(owed[1].state, OwedState::Staged);
        assert_eq!(owed[1].held_by, Some(registration.clone()));
        assert!(!owed[1].sendable);
        assert!(
            !format!("{owed:?}").contains("request:"),
            "no credential is reported"
        );

        // The actor and the node's principal read it; nobody else does.
        let node = on.node.node_context();
        assert_eq!(on.node.owed_acp_as(&node, &on.user).await.unwrap(), owed);
        assert!(denied(
            &on.node.owed_acp_as(&actor(STRANGER), &on.user).await
        ));
        assert!(denied(
            &on.node
                .owed_acp_as(&defra_core::RequestContext::default(), &on.user)
                .await
        ));
        assert!(on
            .node
            .owed_acp_as(&actor(STRANGER), STRANGER)
            .await
            .unwrap()
            .is_empty());

        // The wait says what is left has come to rest, to those who may know.
        assert!(matches!(
            on.node.await_owed_acp_as(&on.caller(), &txn, WAIT).await.unwrap(),
            OwedWait::Parked(entries) if entries == owed
        ));
        assert!(matches!(
            on.node.await_owed_acp_as(&node, &txn, WAIT).await.unwrap(),
            OwedWait::Parked(entries) if entries.len() == 2
        ));
        assert_eq!(
            on.node
                .await_owed_acp_as(&actor(STRANGER), &txn, WAIT)
                .await
                .unwrap(),
            OwedWait::Settled
        );

        // A credential that would not have admitted the registration is
        // refused and records nothing: expired, signed by somebody else,
        // bound to another command, or a plain one presented as bound.
        let present = |credential: SignerCredential| {
            let (on, registration) = (&on, &registration);
            async move {
                on.node
                    .reauthorize_owed_acp_as(&on.caller(), registration, credential)
                    .await
            }
        };
        assert!(unavailable(&present(on.registration(&doc)).await));
        on.remote.lapsed.store(false, Ordering::SeqCst);
        for refused in [
            SignerCredential::None,
            SignerCredential::Delegation(format!("delegation:{STRANGER}")),
            SignerCredential::RequestBound(request_token(STRANGER, &format!("register {doc}"))),
            SignerCredential::RequestBound(granting.clone()),
            SignerCredential::RequestBound(format!("delegation:{}", on.user)),
        ] {
            assert!(unavailable(&present(refused).await));
        }
        // To anyone but its actor and the node the entry does not exist.
        assert!(not_found(
            &on.node
                .reauthorize_owed_acp_as(&actor(STRANGER), &registration, on.registration(&doc))
                .await
        ));
        assert_eq!(on.report().await, owed);
        assert!(on.remote.owners.get(&doc).is_none());

        // One bound to the registration settles it and answers success. The
        // grant behind it then goes under its own delegation.
        assert_eq!(
            present(on.registration(&doc)).await.unwrap(),
            OwedOutcome::Settled
        );
        assert_eq!(
            on.remote.owners.get(&doc).as_deref(),
            Some(on.user.as_str())
        );
        assert_eq!(
            on.remote.take_writes(),
            [(true, doc.clone(), READER.to_string())]
        );
        assert!(on.report().await.is_empty());
        assert_eq!(
            on.node
                .await_owed_acp_as(&on.caller(), &txn, WAIT)
                .await
                .unwrap(),
            OwedWait::Settled
        );
        assert!(not_found(&present(on.registration(&doc)).await));
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_change_waiting_for_authority_takes_a_credential_bound_to_it() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let Attempt { doc, txn, response } = on.create("shared", |_| on.delegation()).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let granting = request_token(&on.user, &format!("set {doc} reader {READER}"));
        assert!(matches!(
            on.grant(&txn, &on.bound(granting.clone()), &doc).await,
            Ok(RelationshipChange::Staged { .. })
        ));
        on.remote.lapsed.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();

        // The registration went under the plain delegation; the grant's own
        // has run out.
        let owed = on.report().await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        assert_eq!(owed[0].held_by, None);
        assert!(owed[0].reason.is_some());
        let change = owed[0].reference();
        assert!(matches!(change, OwedRef::Change { .. }));
        assert!(matches!(
            on.node.await_owed_acp_as(&on.caller(), &txn, WAIT).await.unwrap(),
            OwedWait::Parked(entries) if entries.len() == 1
        ));
        on.remote.lapsed.store(false, Ordering::SeqCst);

        // It is not abandoned: a request sent for it earlier could still land.
        let OwedRef::Change { seq } = change else {
            unreachable!()
        };
        assert!(matches!(
            on.node.abandon_owed_acp_change_as(&on.caller(), seq).await,
            Err(crate::Error::Acp(acp::Error::OwedChangeNotRefused { .. }))
        ));
        // The registration's delegation does not name the grant.
        assert!(unavailable(
            &on.node
                .reauthorize_owed_acp_as(&on.caller(), &change, on.registration(&doc))
                .await
        ));
        assert_eq!(on.report().await, owed);
        assert!(on.remote.take_writes().is_empty());

        assert_eq!(
            on.node
                .reauthorize_owed_acp_as(
                    &on.caller(),
                    &change,
                    SignerCredential::RequestBound(granting)
                )
                .await
                .unwrap(),
            OwedOutcome::Settled
        );
        assert!(on.remote.holds(&doc, READER));
        assert!(on.report().await.is_empty());
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_submission_that_fails_for_an_unreachable_provider_is_reported_and_sent_by_the_wait() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let Attempt { doc, txn, response } = on.create("late", |_| on.delegation()).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        on.remote.down.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();

        let owed = on.report().await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].state, OwedState::Staged);
        assert!(
            owed[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("stub provider is down")),
            "{owed:?}"
        );
        assert!(owed[0].sendable);
        assert!(matches!(
            on.node.await_owed_acp_as(&on.caller(), &txn, WAIT).await.unwrap(),
            OwedWait::Pending { entries, provider_unavailable: true } if entries.len() == 1
        ));

        on.remote.down.store(false, Ordering::SeqCst);
        assert_eq!(
            on.node
                .await_owed_acp_as(&on.caller(), &txn, WAIT)
                .await
                .unwrap(),
            OwedWait::Settled
        );
        assert_eq!(
            on.remote.owners.get(&doc).as_deref(),
            Some(on.user.as_str())
        );
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_retry_of_an_owed_registration_never_reads_the_token_the_process_holds() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let Attempt { doc, txn, response } = on.create("parked", |doc| on.registration(doc)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        on.remote.lapsed.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();
        assert!(on.remote.owners.get(&doc).is_none());

        // The process holds a delegation the provider would take for the
        // user. The retries a start makes, which serve no request, are
        // given the record and nothing else.
        defra_core::signing::set_request_bearer_token(&on.user, format!("delegation:{}", on.user));
        let unbound = on.node.document_acp();
        let _ = unbound.redrive_registration(&on.policy, "rows", &doc).await;
        unbound
            .settle_owed_changes(acp::OwedScope::All)
            .await
            .unwrap();
        defra_core::signing::clear_request_bearer_token(&on.user);

        assert!(on.remote.owners.get(&doc).is_none());
        let owed = on.report().await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        assert_eq!(on.records().await, ["parked"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn only_the_actor_that_staged_a_refused_revocation_abandons_it() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let doc = on
            .committed(on.create("shared", |_| on.delegation()).await)
            .await;
        on.node
            .add_dac_actor_relationship_as(&on.delegating(), "Record", &doc, "reader", READER)
            .await
            .unwrap();
        on.remote.take_writes();
        assert_eq!(on.read_by(READER).await, ["shared"]);

        // The user revokes in a transaction. Before the provider is sent the
        // revocation it gives the document to somebody else, and refuses.
        let txn = on.node.begin_transaction(false).await.unwrap();
        assert!(matches!(
            on.node
                .delete_dac_actor_relationship_change_for_in_txn(
                    &txn,
                    &on.delegating(),
                    "Record",
                    &doc,
                    "reader",
                    READER
                )
                .await,
            Ok(RelationshipChange::Staged { .. })
        ));
        on.remote.owners.insert(doc.clone(), STRANGER.to_string());
        on.node.commit_transaction(&txn).await.unwrap();

        let owed = on.report().await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert!(
            matches!(&owed[0].state, OwedState::Refused { reason } if !reason.is_empty()),
            "{owed:?}"
        );
        let OwedRef::Change { seq } = owed[0].reference() else {
            unreachable!()
        };
        assert!(matches!(
            on.node.await_owed_acp_as(&on.caller(), &txn, WAIT).await.unwrap(),
            OwedWait::Parked(entries) if entries.len() == 1
        ));
        // The provider holds the relationship, and the refused revocation
        // denies on this node all the same.
        assert!(on.remote.holds(&doc, READER));
        assert!(on.read_by(READER).await.is_empty());

        // Not the subject it names, and not the node.
        assert!(not_found(
            &on.node
                .abandon_owed_acp_change_as(&actor(READER), seq)
                .await
        ));
        assert!(denied(
            &on.node
                .abandon_owed_acp_change_as(&on.node.node_context(), seq)
                .await
        ));
        assert!(on.read_by(READER).await.is_empty());

        on.node
            .abandon_owed_acp_change_as(&on.caller(), seq)
            .await
            .unwrap();
        assert!(on.report().await.is_empty());
        assert!(on.remote.take_writes().is_empty());
        // The provider, which refused the revocation, decides again.
        assert_eq!(on.read_by(READER).await, ["shared"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}
