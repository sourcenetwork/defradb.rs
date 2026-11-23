//! The recovery pass over what the gate recorded.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use identity::Did;
use rapidhash::{HashSetExt, RapidHashSet};
use storage::RegolithStore;

use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::external::{ExternalDocumentAcp, OweAgain};
use crate::identity::Identity;
use crate::outbox::{NewOwedChange, OwedAuthority, OwedOp, OwedSubject, OwedTuple};
use crate::permission::DocumentPermission;
use crate::recovery::{
    Observed, RecoveryFact, RecoveryLedger, RecoverySource, RecoveryStep, RecoveryVerifier,
};
use crate::{AcpObject, VoidedEffect};

const OWNER: &str = "did:key:owner";
const READER: &str = "did:key:reader";
const POLICY: &str = "policy";
const USERS: &str = "users";

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

fn reader() -> OwedSubject {
    OwedSubject::Actor { did: did(READER) }
}

fn grant(object: &str, relation: &str, held: bool) -> RecoveryFact {
    RecoveryFact::relationship(POLICY, USERS, object, did(OWNER), relation, reader(), held)
}

fn owned(object: &str) -> RecoveryFact {
    RecoveryFact::registered(POLICY, USERS, object, did(OWNER))
}

/// A provider that registers and grants what it is asked, except the
/// documents and relations it is told to refuse.
#[derive(Default)]
struct Provider {
    refused_documents: RapidHashSet<String>,
    unanswered_documents: RapidHashSet<String>,
    refuses_grants: AtomicBool,
    unreachable: AtomicBool,
    held_by_another: AtomicBool,
}

#[async_trait]
impl DocumentACP for Provider {
    async fn register_doc_object(&self, _: &Did, _: &str, _: &str, doc: &str) -> Result<()> {
        if self.refused_documents.contains(doc) {
            return Err(Error::DocumentAlreadyRegistered(doc.to_owned()));
        }
        if self.unanswered_documents.contains(doc) {
            return Err(Error::ProviderUnavailable("no answer".into()));
        }
        Ok(())
    }

    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> Result<bool> {
        Ok(self.held_by_another.load(Ordering::SeqCst))
    }

    async fn get_doc_owner(&self, _: &str, _: &str, _: &str) -> Result<Option<Did>> {
        Ok(Some(if self.held_by_another.load(Ordering::SeqCst) {
            did("did:key:other")
        } else {
            did(OWNER)
        }))
    }

    async fn check_doc_access(
        &self,
        _: &Identity,
        _: DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn add_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<bool> {
        if self.unreachable.load(Ordering::SeqCst) {
            return Err(Error::ProviderUnavailable("no answer".into()));
        }
        if self.refuses_grants.load(Ordering::SeqCst) {
            return Err(Error::NotOwner {
                operation: "add actor relationship".into(),
            });
        }
        Ok(true)
    }

    async fn delete_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<bool> {
        Ok(true)
    }

    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
}

type Gate = ExternalDocumentAcp<RegolithStore>;

fn gate_over(provider: Provider) -> (Arc<RegolithStore>, Gate) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let gate = ExternalDocumentAcp::new(Arc::new(provider), Arc::clone(&store));
    (store, gate)
}

fn gate() -> (Arc<RegolithStore>, Gate) {
    gate_over(Provider::default())
}

/// Write `facts` as the provider confirmed them.
async fn confirm(store: &RegolithStore, facts: &[RecoveryFact]) {
    let txn = datastore::BasicTxn::new(store, 0, false).await.unwrap();
    let ledger = RecoveryLedger::new(txn.acpstore());
    for fact in facts {
        ledger.confirm(fact).await.unwrap();
    }
    drop(ledger);
    txn.commit().await.unwrap();
}

/// The deployment a pass reads: the objects it holds, each for [`OWNER`],
/// and the relations it holds on every one of them.
struct Deployment {
    term: AtomicU64,
    objects: Option<RapidHashSet<String>>,
    relations: RapidHashSet<String>,
    unobservable: RapidHashSet<String>,
    asked: AtomicU64,
}

impl Deployment {
    /// A deployment that lost everything.
    fn empty() -> Self {
        Self {
            term: AtomicU64::new(1),
            objects: Some(RapidHashSet::new()),
            relations: RapidHashSet::new(),
            unobservable: RapidHashSet::new(),
            asked: AtomicU64::new(0),
        }
    }

    /// A deployment that holds every object, and `relations` on each.
    fn holding(relations: &[&str]) -> Self {
        Self {
            objects: None,
            relations: relations.iter().map(|&name| name.to_owned()).collect(),
            ..Self::empty()
        }
    }
}

#[async_trait]
impl RecoveryVerifier for Deployment {
    async fn term(&self) -> std::result::Result<u64, String> {
        Ok(self.term.load(Ordering::SeqCst))
    }

    async fn object_owner(
        &self,
        _: u64,
        _: &str,
        _: &str,
        object: &str,
    ) -> std::result::Result<Option<String>, String> {
        let held = self
            .objects
            .as_ref()
            .is_none_or(|objects| objects.contains(object));
        Ok(held.then(|| OWNER.to_owned()))
    }

    async fn relationship(
        &self,
        _: u64,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        relation: &str,
        _: &OwedSubject,
    ) -> std::result::Result<Observed, String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(if self.unobservable.contains(relation) {
            Observed::Unobservable("the deployment cannot be asked".into())
        } else if self.relations.contains(relation) {
            Observed::Held
        } else {
            Observed::Absent
        })
    }
}

/// Run the pass the way the provider does: every page handed over and
/// recorded before the next, until the source says it is over.
async fn pass(gate: &Gate, deployment: &Deployment) -> (Vec<VoidedEffect>, Vec<String>, usize) {
    let (mut effects, mut unrestored, mut pages) = (Vec::new(), Vec::new(), 0);
    while let RecoveryStep::Page(page) = gate.next(deployment).await.unwrap() {
        pages += 1;
        assert!(pages < 100, "the pass does not end");
        effects.extend(page.effects);
        unrestored.extend(page.unrestored);
        gate.delivered(page.cursor).await.unwrap();
    }
    (effects, unrestored, pages)
}

fn is_registration(effect: &VoidedEffect, of: &str) -> bool {
    matches!(effect, VoidedEffect::Registration { object, .. } if object == of)
}

fn is_change(effect: &VoidedEffect, of: &str, relation: &str, op: OwedOp) -> bool {
    matches!(effect, VoidedEffect::Change(change)
        if change.tuple.object == of && change.tuple.relation == relation && change.op == op)
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn a_pass_owes_what_the_deployment_lost_registration_first_and_then_ends() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[grant("bae-1", "reader", true), owned("bae-1")]).await;

        let (effects, unrestored, pages) = pass(&gate, &Deployment::empty()).await;
        assert_eq!(pages, 1);
        assert_eq!(effects.len(), 2);
        assert!(is_registration(&effects[0], "bae-1"));
        assert!(is_change(&effects[1], "bae-1", "reader", OwedOp::Add));
        assert!(unrestored.is_empty());
        assert!(
            matches!(
                gate.next(&Deployment::empty()).await.unwrap(),
                RecoveryStep::Done { term: 1 }
            ),
            "a finished pass stays finished, and names its term"
        );
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn a_pass_checks_the_records_past_its_first_page() {
    workers::block_on(async {
        let (store, gate) = gate();
        let mut facts = Vec::new();
        for index in 0..3000 {
            let object = format!("bae-{index:04}");
            facts.push(owned(&object));
            facts.push(grant(&object, "reader", true));
        }
        confirm(&store, &facts).await;

        let (effects, _, pages) = pass(&gate, &Deployment::empty()).await;
        assert!(pages > 1, "the ledger is larger than one page");
        assert_eq!(effects.len(), 6000);
        let registered: RapidHashSet<&str> = effects
            .iter()
            .filter_map(|effect| match effect {
                VoidedEffect::Registration { object, .. } => Some(object.as_str()),
                VoidedEffect::Change(_) => None,
            })
            .collect();
        assert_eq!(registered.len(), 3000, "every object once");
        let granted: RapidHashSet<&str> = effects
            .iter()
            .filter_map(|effect| match effect {
                VoidedEffect::Change(change) => {
                    assert_eq!(change.op, OwedOp::Add);
                    assert_eq!(change.tuple.policy_id, POLICY);
                    assert_eq!(change.tuple.resource, USERS);
                    assert_eq!(change.tuple.relation, "reader");
                    assert_eq!(change.tuple.subject, reader());
                    assert_eq!(change.actor, did(OWNER));
                    Some(change.tuple.object.as_str())
                }
                VoidedEffect::Registration { .. } => None,
            })
            .collect();
        assert_eq!(granted, registered, "and every grant once");
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn a_page_not_recorded_is_read_again_and_a_recorded_one_is_not() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[owned("bae-1")]).await;
        let deployment = Deployment::empty();

        let RecoveryStep::Page(first) = gate.next(&deployment).await.unwrap() else {
            panic!("a page");
        };
        let RecoveryStep::Page(again) = gate.next(&deployment).await.unwrap() else {
            panic!("the page again");
        };
        assert_eq!(first.effects, again.effects);
        gate.delivered(again.cursor).await.unwrap();
        assert!(matches!(
            gate.next(&deployment).await.unwrap(),
            RecoveryStep::Done { .. }
        ));
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn a_new_term_starts_the_pass_again() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[owned("bae-1")]).await;
        let deployment = Deployment::empty();
        let (effects, _, _) = pass(&gate, &deployment).await;
        assert_eq!(effects.len(), 1);

        deployment.term.store(2, Ordering::SeqCst);
        let (effects, _, pages) = pass(&gate, &deployment).await;
        assert_eq!(pages, 1);
        assert!(
            is_registration(&effects[0], "bae-1"),
            "the new term is checked from the first record"
        );
    })
    .unwrap()
}

#[test]
fn a_confirmed_delete_is_owed_when_the_deployment_still_grants() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[owned("bae-1"), grant("bae-1", "reader", false)]).await;

        let (effects, _, _) = pass(&gate, &Deployment::holding(&["reader"])).await;
        assert_eq!(effects.len(), 1);
        assert!(is_change(&effects[0], "bae-1", "reader", OwedOp::Delete));
    })
    .unwrap()
}

#[test]
fn what_the_deployment_holds_as_recorded_owes_nothing() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(
            &store,
            &[
                owned("bae-1"),
                grant("bae-1", "reader", true),
                grant("bae-1", "updater", false),
            ],
        )
        .await;

        let (effects, unrestored, _) = pass(&gate, &Deployment::holding(&["reader"])).await;
        assert!(effects.is_empty());
        assert!(unrestored.is_empty());
    })
    .unwrap()
}

#[test]
fn a_relationship_the_deployment_cannot_be_asked_about_is_reported_and_the_pass_goes_on() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(
            &store,
            &[
                owned("bae-1"),
                grant("bae-1", "auditor", true),
                grant("bae-1", "reader", true),
            ],
        )
        .await;
        let mut deployment = Deployment::holding(&[]);
        deployment.unobservable.insert("auditor".into());

        let (effects, unrestored, _) = pass(&gate, &deployment).await;
        assert_eq!(
            unrestored,
            vec!["policy/users/bae-1 auditor: the deployment cannot be asked".to_owned()]
        );
        assert_eq!(effects.len(), 1);
        assert!(is_change(&effects[0], "bae-1", "reader", OwedOp::Add));
    })
    .unwrap()
}

#[test]
fn a_lost_registration_owes_its_grants_without_asking_about_them() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(
            &store,
            &[
                owned("bae-1"),
                grant("bae-1", "reader", true),
                grant("bae-1", "updater", false),
            ],
        )
        .await;
        let deployment = Deployment::empty();

        let (effects, _, _) = pass(&gate, &deployment).await;
        assert_eq!(
            effects.len(),
            2,
            "the deleted grant is gone with the object"
        );
        assert_eq!(deployment.asked.load(Ordering::SeqCst), 0);
    })
    .unwrap()
}

#[test]
fn a_grant_without_a_recorded_registration_is_reported() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[grant("bae-1", "reader", true)]).await;

        let (effects, unrestored, _) = pass(&gate, &Deployment::empty()).await;
        assert!(effects.is_empty());
        assert_eq!(
            unrestored,
            vec!["policy/users/bae-1 reader: no registration was recorded".to_owned()]
        );
    })
    .unwrap()
}

#[test]
fn an_object_another_owner_holds_is_reported_and_not_owed() {
    workers::block_on(async {
        let (store, gate) = gate();
        let theirs = RecoveryFact::registered(POLICY, USERS, "bae-1", did("did:key:other"));
        confirm(&store, &[theirs, grant("bae-1", "reader", true)]).await;

        let (effects, unrestored, _) = pass(&gate, &Deployment::holding(&[])).await;
        assert!(effects.is_empty());
        assert_eq!(
            unrestored,
            vec![format!(
                "policy/users/bae-1: the deployment holds it for another owner, {OWNER}"
            )]
        );
    })
    .unwrap()
}

#[test]
fn an_unregistered_object_is_not_rebuilt() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(
            &store,
            &[
                RecoveryFact::unregistered(POLICY, USERS, "bae-1"),
                grant("bae-1", "reader", true),
            ],
        )
        .await;

        let (effects, unrestored, _) = pass(&gate, &Deployment::empty()).await;
        assert!(effects.is_empty());
        assert!(unrestored.is_empty());

        let deployment = Deployment::holding(&[]);
        deployment.term.store(2, Ordering::SeqCst);
        let (effects, unrestored, _) = pass(&gate, &deployment).await;
        assert!(effects.is_empty());
        assert_eq!(
            unrestored,
            vec![
                "policy/users/bae-1: the deployment holds a registration this node removed"
                    .to_owned()
            ]
        );
    })
    .unwrap()
}

#[test]
fn an_unanswered_delete_is_owed_and_an_unanswered_grant_is_only_reported() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[owned("bae-1"), grant("bae-1", "reader", true)]).await;
        gate.recovery_prepare(&[grant("bae-1", "updater", true)])
            .await
            .unwrap();
        gate.recovery_prepare(&[grant("bae-1", "reader", true)])
            .await
            .unwrap();
        gate.recovery_prepare(&[grant("bae-1", "reader", false)])
            .await
            .unwrap();

        let (effects, unrestored, _) = pass(&gate, &Deployment::holding(&["reader"])).await;
        assert_eq!(effects.len(), 1, "{effects:?}");
        assert!(is_change(&effects[0], "bae-1", "reader", OwedOp::Delete));
        assert_eq!(
            unrestored,
            vec!["policy/users/bae-1 updater: the provider never answered this grant".to_owned()]
        );
    })
    .unwrap()
}

#[test]
fn an_unanswered_registration_is_reported_with_nothing_owed_on_its_object() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[grant("bae-1", "reader", true)]).await;
        gate.recovery_prepare(&[owned("bae-1")]).await.unwrap();

        let (effects, unrestored, _) = pass(&gate, &Deployment::empty()).await;
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(
            unrestored,
            vec!["policy/users/bae-1: the provider never answered its registration".to_owned()]
        );

        let deployment = Deployment::holding(&["reader"]);
        deployment.term.store(2, Ordering::SeqCst);
        let (effects, unrestored, _) = pass(&gate, &deployment).await;
        assert!(effects.is_empty() && unrestored.is_empty(), "it was taken");
    })
    .unwrap()
}

#[test]
fn a_grant_the_provider_refused_is_not_owed_by_a_pass() {
    workers::block_on(async {
        let provider = Provider::default();
        provider.refuses_grants.store(true, Ordering::SeqCst);
        let (store, gate) = gate_over(provider);
        confirm(&store, &[owned("bae-1")]).await;

        let refused = gate
            .add_actor_relationship(
                &did(OWNER),
                &did(READER),
                POLICY,
                USERS,
                "bae-1",
                "reader",
                &[],
            )
            .await;
        assert!(matches!(refused, Err(Error::NotOwner { .. })));

        let (effects, unrestored, _) = pass(&gate, &Deployment::holding(&[])).await;
        assert!(effects.is_empty(), "{effects:?}");
        assert!(unrestored.is_empty(), "{unrestored:?}");
    })
    .unwrap()
}

#[test]
fn a_grant_the_provider_never_answered_is_reported_and_not_owed() {
    workers::block_on(async {
        let provider = Provider::default();
        provider.unreachable.store(true, Ordering::SeqCst);
        let (store, gate) = gate_over(provider);
        confirm(&store, &[owned("bae-1")]).await;

        let unanswered = gate
            .add_actor_relationship(
                &did(OWNER),
                &did(READER),
                POLICY,
                USERS,
                "bae-1",
                "reader",
                &[],
            )
            .await;
        assert!(matches!(unanswered, Err(Error::ProviderUnavailable(_))));

        let (effects, unrestored, _) = pass(&gate, &Deployment::holding(&[])).await;
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(
            unrestored,
            vec!["policy/users/bae-1 reader: the provider never answered this grant".to_owned()]
        );
    })
    .unwrap()
}

#[test]
fn a_delete_the_deployment_cannot_be_asked_about_is_owed_again() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[owned("bae-1"), grant("bae-1", "auditor", false)]).await;
        let mut deployment = Deployment::holding(&["auditor"]);
        deployment.unobservable.insert("auditor".into());

        let (effects, unrestored, _) = pass(&gate, &deployment).await;
        assert_eq!(effects.len(), 1, "{effects:?}");
        assert!(is_change(&effects[0], "bae-1", "auditor", OwedOp::Delete));
        assert!(unrestored.is_empty(), "{unrestored:?}");
    })
    .unwrap()
}

#[test]
fn a_confirmed_delete_stands_while_a_later_grant_is_unanswered() {
    workers::block_on(async {
        let (store, gate) = gate();
        confirm(&store, &[owned("bae-1"), grant("bae-1", "reader", false)]).await;
        gate.recovery_prepare(&[grant("bae-1", "reader", true)])
            .await
            .unwrap();

        let (effects, unrestored, _) = pass(&gate, &Deployment::holding(&["reader"])).await;
        assert_eq!(effects.len(), 1, "{effects:?}");
        assert!(is_change(&effects[0], "bae-1", "reader", OwedOp::Delete));
        assert_eq!(
            unrestored,
            vec!["policy/users/bae-1 reader: the provider never answered this grant".to_owned()]
        );
    })
    .unwrap()
}

#[test]
fn a_registration_another_owner_held_first_is_not_recorded_as_this_nodes() {
    workers::block_on(async {
        let provider = Provider::default();
        provider.held_by_another.store(true, Ordering::SeqCst);
        let (_, gate) = gate_over(provider);
        gate.owe_again(&[VoidedEffect::Registration {
            creator: did(OWNER),
            policy_id: POLICY.into(),
            resource: USERS.into(),
            object: "bae-1".into(),
            authority: OwedAuthority::NodeKey,
            request_bound: false,
        }])
        .await
        .unwrap();
        gate.settle_owed().await;

        let (effects, unrestored, _) = pass(&gate, &Deployment::empty()).await;
        assert!(effects.is_empty(), "{effects:?}");
        assert!(unrestored.is_empty(), "{unrestored:?}");
    })
    .unwrap()
}

#[test]
fn a_change_made_through_the_gate_is_recorded_once_the_provider_confirms_it() {
    workers::block_on(async {
        let (_, gate) = gate();
        gate.register_doc_object(&did(OWNER), POLICY, USERS, "bae-1")
            .await
            .unwrap();
        gate.add_actor_relationship(
            &did(OWNER),
            &did(READER),
            POLICY,
            USERS,
            "bae-1",
            "reader",
            &[],
        )
        .await
        .unwrap();
        gate.delete_actor_relationship(
            &did(OWNER),
            &did(READER),
            POLICY,
            USERS,
            "bae-1",
            "updater",
            &[],
        )
        .await
        .unwrap();

        let (effects, _, _) = pass(&gate, &Deployment::holding(&["updater"])).await;
        assert_eq!(effects.len(), 2);
        assert!(is_change(&effects[0], "bae-1", "reader", OwedOp::Add));
        assert!(is_change(&effects[1], "bae-1", "updater", OwedOp::Delete));
    })
    .unwrap()
}

#[test]
fn a_list_that_fails_part_way_keeps_the_registrations_before_the_failure() {
    workers::block_on(async {
        let mut provider = Provider::default();
        provider.refused_documents.insert("bae-2".into());
        let (_, gate) = gate_over(provider);
        let objects: Vec<AcpObject<'_>> = ["bae-1", "bae-2", "bae-3"]
            .into_iter()
            .map(|object_id| AcpObject {
                policy_id: POLICY,
                resource: USERS,
                object_id,
            })
            .collect();

        let failure = gate
            .register_objects(&did(OWNER), &objects)
            .await
            .expect_err("the second registration is refused");
        assert_eq!(failure.index, 1);

        let (effects, unrestored, _) = pass(&gate, &Deployment::empty()).await;
        assert_eq!(effects.len(), 1, "{effects:?}");
        assert!(is_registration(&effects[0], "bae-1"));
        assert!(unrestored.is_empty(), "{unrestored:?}");
    })
    .unwrap()
}

#[test]
fn a_list_cut_short_by_an_unreachable_provider_keeps_what_it_may_have_taken() {
    workers::block_on(async {
        let mut provider = Provider::default();
        provider.unanswered_documents.insert("bae-2".into());
        let (_, gate) = gate_over(provider);
        let objects: Vec<AcpObject<'_>> = ["bae-1", "bae-2", "bae-3"]
            .into_iter()
            .map(|object_id| AcpObject {
                policy_id: POLICY,
                resource: USERS,
                object_id,
            })
            .collect();

        let failure = gate
            .register_objects(&did(OWNER), &objects)
            .await
            .expect_err("the second registration is not answered");
        assert_eq!(failure.index, 1);

        let (effects, unrestored, _) = pass(&gate, &Deployment::empty()).await;
        assert_eq!(effects.len(), 1, "{effects:?}");
        assert!(is_registration(&effects[0], "bae-1"));
        assert_eq!(
            unrestored,
            vec![
                "policy/users/bae-2: the provider never answered its registration".to_owned(),
                "policy/users/bae-3: the provider never answered its registration".to_owned(),
            ],
            "the list may have gone through whole from where the answer was lost"
        );
    })
    .unwrap()
}

#[test]
fn a_list_whose_answer_is_lost_at_its_start_keeps_every_intent() {
    workers::block_on(async {
        let mut provider = Provider::default();
        provider.unanswered_documents.insert("bae-1".into());
        let (_, gate) = gate_over(provider);
        let objects: Vec<AcpObject<'_>> = ["bae-1", "bae-2", "bae-3"]
            .into_iter()
            .map(|object_id| AcpObject {
                policy_id: POLICY,
                resource: USERS,
                object_id,
            })
            .collect();

        let failure = gate
            .register_objects(&did(OWNER), &objects)
            .await
            .expect_err("the list is not answered");
        assert_eq!(failure.index, 0);

        let (effects, unrestored, _) = pass(&gate, &Deployment::empty()).await;
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(unrestored.len(), 3, "{unrestored:?}");
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn what_the_outbox_settles_is_recorded_and_owed_again_when_the_deployment_loses_it() {
    workers::block_on(async {
        let (_, gate) = gate();
        let registration = VoidedEffect::Registration {
            creator: did(OWNER),
            policy_id: POLICY.into(),
            resource: USERS.into(),
            object: "bae-1".into(),
            authority: OwedAuthority::NodeKey,
            request_bound: false,
        };
        let change = |relation: &str, op| {
            VoidedEffect::Change(NewOwedChange {
                txn: None,
                tuple: OwedTuple {
                    policy_id: POLICY.into(),
                    resource: USERS.into(),
                    object: "bae-1".into(),
                    relation: relation.into(),
                    subject: reader(),
                },
                op,
                actor: did(OWNER),
                authority: OwedAuthority::NodeKey,
                authority_checked: true,
            })
        };
        gate.owe_again(&[
            registration,
            change("reader", OwedOp::Add),
            change("updater", OwedOp::Delete),
        ])
        .await
        .unwrap();
        let (effects, _, _) = pass(&gate, &Deployment::empty()).await;
        assert!(effects.is_empty(), "nothing is recorded before it settles");

        gate.settle_owed().await;
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());

        let deployment = Deployment::empty();
        deployment.term.store(2, Ordering::SeqCst);
        let (effects, _, _) = pass(&gate, &deployment).await;
        assert_eq!(effects.len(), 2, "{effects:?}");
        assert!(is_registration(&effects[0], "bae-1"));
        assert!(is_change(&effects[1], "bae-1", "reader", OwedOp::Add));

        let deployment = Deployment::holding(&["updater"]);
        deployment.term.store(3, Ordering::SeqCst);
        let (effects, _, _) = pass(&gate, &deployment).await;
        assert_eq!(effects.len(), 2, "{effects:?}");
        assert!(is_change(&effects[0], "bae-1", "reader", OwedOp::Add));
        assert!(is_change(&effects[1], "bae-1", "updater", OwedOp::Delete));
    })
    .unwrap()
}
