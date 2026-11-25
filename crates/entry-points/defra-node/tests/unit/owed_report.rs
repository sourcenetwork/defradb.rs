//! What a node owes a remote document ACP provider after a commit, as its
//! callers act on it: the report, the wait for one transaction, a fresh
//! credential for one owed registration or change, and abandoning a refused
//! change.

use std::sync::atomic::Ordering;
use std::time::Duration;

use acp::{OwedHold, OwedOutcome, OwedRef, OwedState, OwedWait, OwedWhat};
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
        assert_eq!(
            owed[0].txn,
            txn.as_str()
                .parse::<defra_core::TxnId>()
                .ok()
                .map(|id| id.route)
        );
        assert!(!owed[0].sendable);
        // The grant waits behind it, neither sent nor parked.
        assert!(matches!(owed[1].what, OwedWhat::Change { .. }));
        assert_eq!(owed[1].state, OwedState::Staged);
        assert_eq!(owed[1].held_by, Some(OwedHold::Registration));
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
        // Nothing was recorded or sent. The recorded delegation counts
        // again (the stub's expiry is a switch), and the report says a
        // submission would now move both entries.
        let unchanged = on.report().await;
        assert_eq!(unchanged.len(), 2);
        for (now, before) in unchanged.iter().zip(&owed) {
            assert_eq!((&now.state, &now.reason), (&before.state, &before.reason));
            assert!(now.sendable);
        }
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
        // Nor is a request that carries no bearer of its own, whoever makes
        // it: it does not lend the creator's process token to the creator's
        // parked registration.
        for caller in [on.user.as_str(), STRANGER] {
            let touched = on
                .node
                .add_dac_actor_relationship(caller, "Record", &doc, "reader", READER)
                .await;
            assert!(touched.is_err(), "{touched:?}");
            assert!(on.remote.owners.get(&doc).is_none(), "as {caller}");
        }
        let decided = on
            .node
            .document_acp()
            .bind_request(&actor(&on.user))
            .expect("the gate serves a request")
            .create_access_decision(
                &acp::Identity::Authenticated(Did::new(&on.user).unwrap()),
                &on.policy,
                "rows",
                &doc,
                "read",
            )
            .await;
        assert!(decided.is_err(), "{decided:?}");
        // And it is no fresh credential: one is handed in by a request that
        // carries it.
        let user = Did::new(&on.user).unwrap();
        let registration = on.report().await[0].reference();
        for gate in [
            on.node.document_acp(),
            on.node
                .document_acp()
                .bind_request(&actor(&on.user))
                .unwrap(),
        ] {
            assert!(matches!(
                gate.reauthorize_owed(&user, &registration).await,
                Err(acp::Error::OwedAuthorityUnavailable { .. })
            ));
        }
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

        // Not the subject it names, and not the node, which abandons only
        // what it staged itself: to both the change does not exist.
        for other in [actor(READER), on.node.node_context()] {
            assert!(not_found(
                &on.node.abandon_owed_acp_change_as(&other, seq).await
            ));
        }
        assert!(denied(
            &on.node
                .abandon_owed_acp_change_as(&defra_core::RequestContext::default(), seq)
                .await
        ));
        assert!(on.read_by(READER).await.is_empty());

        on.node
            .abandon_owed_acp_change_as(&on.caller(), seq)
            .await
            .unwrap();
        // What is left says the provider does not hold the revocation, and
        // is told once to whoever waits for the transaction.
        let left = on.report().await;
        assert_eq!(left.len(), 1, "{left:?}");
        assert!(left[0].discarded);
        assert!(matches!(
            on.node.await_owed_acp_as(&on.caller(), &txn, WAIT).await.unwrap(),
            OwedWait::Parked(entries) if entries.len() == 1 && entries[0].discarded
        ));
        assert!(on.report().await.is_empty());
        assert!(on.remote.take_writes().is_empty());
        // The provider, which refused the revocation, decides again.
        assert_eq!(on.read_by(READER).await, ["shared"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn the_node_abandons_a_refused_change_it_staged_itself() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let node = on.node.node_context();
        let principal = node
            .identity()
            .expect("the node acts as its identity")
            .to_owned();
        on.remote.keys.insert(principal.clone(), ());
        let created = on
            .node
            .execute_request(
                query::QueryRequest::new(
                    "mutation { add_Record(input: {name: \"own\"}) { _docID } }",
                )
                .with_identity(Some(Did::new(&principal).unwrap())),
            )
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.unwrap()["add_Record"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        on.node
            .add_dac_actor_relationship_as(&node, "Record", &doc, "reader", READER)
            .await
            .unwrap();

        let txn = on.node.begin_transaction(false).await.unwrap();
        assert!(matches!(
            on.node
                .delete_dac_actor_relationship_change_for_in_txn(
                    &txn, &node, "Record", &doc, "reader", READER
                )
                .await,
            Ok(RelationshipChange::Staged { .. })
        ));
        on.remote.owners.insert(doc.clone(), STRANGER.to_string());
        on.node.commit_transaction(&txn).await.unwrap();
        let owed = on.node.owed_acp_as(&node, &principal).await.unwrap();
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert!(matches!(owed[0].state, OwedState::Refused { .. }));
        let OwedRef::Change { seq } = owed[0].reference() else {
            unreachable!()
        };
        assert!(on.read_by(READER).await.is_empty());

        on.node
            .abandon_owed_acp_change_as(&node, seq)
            .await
            .unwrap();
        assert_eq!(on.read_by(READER).await, ["own"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_fresh_credential_the_provider_cannot_verify_is_not_recorded() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        // The node holds the user's key for the write, and loses it before
        // the provider can be reached.
        on.remote.keys.insert(on.user.clone(), ());
        let Attempt { doc, txn, response } = on.create("owed", |_| SignerCredential::None).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        on.remote.down.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();
        on.remote.down.store(false, Ordering::SeqCst);
        on.remote.keys.remove(&on.user);
        let registration = on.report().await[0].reference();

        // A provider that is passed its caller's token as it is gives the
        // node nothing to check a delegation against. A write is not
        // admitted on one there, and neither is an owed entry sent on one.
        on.remote.unverifying.store(true, Ordering::SeqCst);
        for unverified in [
            SignerCredential::Delegation(format!("delegation:{STRANGER}")),
            SignerCredential::Delegation("anything".into()),
        ] {
            assert!(unavailable(
                &on.node
                    .reauthorize_owed_acp_as(&on.caller(), &registration, unverified)
                    .await
            ));
        }
        assert!(on.remote.owners.get(&doc).is_none());
        assert_eq!(on.report().await.len(), 1);

        // A key the node holds again is checked by the node itself.
        on.remote.keys.insert(on.user.clone(), ());
        assert_eq!(
            on.node
                .reauthorize_owed_acp_as(&on.caller(), &registration, SignerCredential::None)
                .await
                .unwrap(),
            OwedOutcome::Settled
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
fn a_wait_sends_a_parked_registration_that_its_changes_delegation_serves() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        // This test drives settlement explicitly so the background drainer cannot
        // use the change's delegation before the wait observes the parked registration.
        on.node.background_tasks().shutdown().await;
        let Attempt { doc, txn, response } = on.create("shared", |doc| on.registration(doc)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        // The grant beside it carries the user's plain delegation.
        assert!(matches!(
            on.grant(&txn, &on.delegating(), &doc).await,
            Ok(RelationshipChange::Staged { .. })
        ));
        on.remote.down.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();
        on.remote.down.store(false, Ordering::SeqCst);

        // The registration's own delegation runs out, and a retry that
        // serves no request parks it.
        on.remote.lapsed.store(true, Ordering::SeqCst);
        let _ = on
            .node
            .document_acp()
            .redrive_registration(&on.policy, "rows", &doc)
            .await;
        let owed = on.report().await;
        assert_eq!(owed.len(), 2, "{owed:?}");
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        // A submission would still settle it, with the grant's delegation,
        // and the report says so.
        assert!(owed[0].sendable && owed[1].sendable, "{owed:?}");

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
        assert!(on.remote.holds(&doc, READER));
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_request_that_carries_no_bearer_registers_nothing_under_a_token_the_process_holds() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        // An embedder binds a delegation to the user in the process. No
        // request presents it.
        defra_core::signing::set_request_bearer_token(&on.user, format!("delegation:{}", on.user));

        // A create that carries no bearer commits, and its registration is
        // neither sent at the commit nor recorded with any authority: it
        // waits for a credential and is reported.
        let created = on
            .node
            .execute_request(
                query::QueryRequest::new(
                    "mutation { add_Record(input: {name: \"owed\"}) { _docID } }",
                )
                .with_identity(Some(Did::new(&on.user).unwrap())),
            )
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.unwrap()["add_Record"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(on.remote.owners.get(&doc).is_none());
        let owed = on.report().await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].authority, None, "{owed:?}");
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        assert_eq!(on.records().await, ["owed"]);

        // No retry sends it either, and a relationship call that carries no
        // bearer is refused the same way.
        let _ = on
            .node
            .document_acp()
            .redrive_registration(&on.policy, "rows", &doc)
            .await;
        let granted = on
            .node
            .add_dac_actor_relationship(&on.user, "Record", &doc, "reader", READER)
            .await;
        assert!(granted.is_err(), "{granted:?}");
        assert!(on.remote.owners.get(&doc).is_none());
        assert!(on.remote.take_writes().is_empty());
        defra_core::signing::clear_request_bearer_token(&on.user);

        // A credential handed in for it settles it.
        assert_eq!(
            on.node
                .reauthorize_owed_acp_as(&on.caller(), &owed[0].reference(), on.delegation())
                .await
                .unwrap(),
            OwedOutcome::Settled
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
fn a_pass_sends_a_registration_with_whichever_owed_delegation_the_report_found() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let Attempt { doc, txn, response } = on.create("shared", |doc| on.registration(doc)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        // The first grant carries a delegation bound to it, the second, of
        // another relationship, the user's plain one.
        let granting = request_token(&on.user, &format!("set {doc} reader {READER}"));
        assert!(matches!(
            on.grant(&txn, &on.bound(granting), &doc).await,
            Ok(RelationshipChange::Staged { .. })
        ));
        assert!(matches!(
            on.node
                .add_dac_actor_relationship_change_for_in_txn(
                    &txn,
                    &on.delegating(),
                    "Record",
                    &doc,
                    "reader",
                    STRANGER
                )
                .await,
            Ok(RelationshipChange::Staged { .. })
        ));
        on.remote.down.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();
        on.remote.down.store(false, Ordering::SeqCst);

        // Both bound delegations run out: the registration's and the first
        // grant's. Only the second grant's delegation still serves.
        on.remote.lapsed.store(true, Ordering::SeqCst);
        let _ = on
            .node
            .document_acp()
            .redrive_registration(&on.policy, "rows", &doc)
            .await;
        let owed = on.report().await;
        assert_eq!(owed.len(), 3, "{owed:?}");
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        assert!(owed[0].sendable, "{owed:?}");

        // What the report calls sendable a pass sends: the registration
        // goes with the second grant's delegation, that grant follows, and
        // the first is parked on its own account.
        let waited = on
            .node
            .await_owed_acp_as(&on.caller(), &txn, Duration::from_secs(3))
            .await
            .unwrap();
        assert_eq!(
            on.remote.owners.get(&doc).as_deref(),
            Some(on.user.as_str())
        );
        assert!(on.remote.holds(&doc, STRANGER));
        assert!(!on.remote.holds(&doc, READER));
        assert!(
            matches!(&waited, OwedWait::Parked(entries)
                if entries.len() == 1 && entries[0].state == OwedState::AwaitingAuthority),
            "{waited:?}"
        );
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}
