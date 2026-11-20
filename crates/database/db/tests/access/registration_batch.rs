//! The registrations one transaction owes an external provider are settled
//! together: one provider submission per creator, applied all or none. A
//! refused member is answered as it would be alone, a lost answer is not
//! sent again, and each creator's registrations go in a list of its own.

use std::sync::atomic::Ordering;

use acp::outbox::{OwedFilter, OwedState};
use acp::{AcpObject, DocumentACP};
use identity::Did;
use query::{QueryExecutor, TransactionHandle};

use super::external_registration::{alice, bob, readable, request, POLICY, RESOURCE};
use super::external_staging::Staging;

/// How many documents the transactions here create.
const MANY: usize = 10;

impl Staging {
    /// `owner` creates `name` in `handle`'s transaction.
    async fn create_in(&self, owner: Did, name: &str, handle: &TransactionHandle) -> String {
        self.create(owner, name, Some(handle)).await
    }

    /// Every document `owner` created and is still owed, by DocID.
    async fn owed_docs(&self, owner: &Did) -> Vec<(String, OwedState)> {
        self.gate
            .owed_entries(OwedFilter::Actor(owner))
            .await
            .unwrap()
            .into_iter()
            .map(|entry| (entry.object, entry.state))
            .collect()
    }

    /// The provider submissions made so far.
    fn submissions(&self) -> usize {
        self.remote.submissions.load(Ordering::SeqCst)
    }
}

fn objects(docs: &[String]) -> Vec<AcpObject<'_>> {
    docs.iter()
        .map(|doc| AcpObject {
            policy_id: POLICY,
            resource: RESOURCE,
            object_id: doc,
        })
        .collect()
}

#[test]
fn the_creates_of_one_transaction_are_registered_in_one_submission() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let handle = staging.begin().await;
        let mut docs = Vec::new();
        for n in 0..MANY {
            docs.push(
                staging
                    .create_in(alice(), &format!("doc {n}"), &handle)
                    .await,
            );
        }
        assert_eq!(
            staging.submissions(),
            0,
            "nothing is sent before the commit"
        );
        staging.commit(&handle).await;

        assert_eq!(staging.submissions(), 1);
        assert_eq!(staging.remote.lists(), vec![docs.clone()]);
        for doc in &docs {
            assert_eq!(staging.remote.owner(doc), Some(alice()));
        }
        assert!(staging.owed_docs(&alice()).await.is_empty());
    })
    .unwrap()
}

#[test]
fn the_documents_one_mutation_creates_are_registered_in_one_submission() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let inputs: Vec<String> = (0..MANY)
            .map(|n| format!(r#"{{name: "doc {n}"}}"#))
            .collect();
        let created = staging
            .node
            .runner
            .execute(request(
                &format!(
                    "mutation {{ add_Users(input: [{}]) {{ _docID }} }}",
                    inputs.join(", ")
                ),
                Some(alice()),
            ))
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let docs: Vec<String> = created.data.unwrap()["add_Users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|doc| doc["_docID"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(docs.len(), MANY);

        assert_eq!(staging.submissions(), 1);
        for doc in &docs {
            assert_eq!(staging.remote.owner(doc), Some(alice()));
        }
    })
    .unwrap()
}

#[test]
fn one_registration_alone_is_sent_as_before() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "alone", None).await;
        assert_eq!(staging.submissions(), 1);
        assert!(staging.remote.lists().is_empty(), "no list for one");
        assert_eq!(staging.remote.owner(&doc), Some(alice()));
    })
    .unwrap()
}

#[test]
fn a_refused_member_is_answered_as_alone_and_the_others_are_registered() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let mut docs = Vec::new();
        for n in 0..5 {
            docs.push(
                staging
                    .create_unregistered(alice(), &format!("doc {n}"))
                    .await,
            );
        }
        let refused = docs[2].clone();
        staging.remote.refuse_object(&refused);
        let before = staging.submissions();

        let outcomes = staging.gate.redrive_registrations(&objects(&docs)).await;

        // Each document is answered for: only the refused one failed, with
        // the provider's own refusal, and its record is parked as refused
        // exactly as when it is sent alone.
        for (doc, outcome) in docs.iter().zip(&outcomes) {
            if *doc == refused {
                assert!(
                    matches!(outcome, Err(acp::Error::PermissionDenied(reason)) if reason.contains("refuses")),
                    "{outcome:?}"
                );
            } else {
                assert!(outcome.is_ok(), "{doc}: {outcome:?}");
                assert_eq!(staging.remote.owner(doc), Some(alice()));
            }
        }
        assert_eq!(staging.remote.owner(&refused), None);
        let owed = staging.owed_docs(&alice()).await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].0, refused);
        assert!(
            matches!(&owed[0].1, OwedState::Refused { reason } if reason.contains("refuses")),
            "{owed:?}"
        );
        // The refused list, its members alone up to the refused one, then
        // the rest in a list of their own.
        assert_eq!(staging.submissions() - before, 1 + 3 + 1);
        assert_eq!(
            staging.remote.lists(),
            vec![docs.clone(), docs[3..].to_vec()]
        );
        // Nobody but the creator reads the one still owed.
        assert!(readable(&staging.node.runner, Some(bob())).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_list_whose_answer_was_lost_is_not_sent_again() {
    workers::block_on(async {
        let staging = Staging::new().await;
        staging
            .remote
            .lose_list_answer
            .store(true, Ordering::SeqCst);
        let handle = staging.begin().await;
        let mut docs = Vec::new();
        for n in 0..MANY {
            docs.push(
                staging
                    .create_in(alice(), &format!("doc {n}"), &handle)
                    .await,
            );
        }
        staging.commit(&handle).await;

        // The provider applied the list; the node did not hear it.
        assert_eq!(staging.submissions(), 1);
        assert_eq!(staging.owed_docs(&alice()).await.len(), MANY);
        for doc in &docs {
            assert_eq!(staging.remote.owner(doc), Some(alice()));
        }

        // The next settlement finds every object held and sends nothing.
        let report = staging.gate.redrive_owed().await.unwrap();
        assert_eq!((report.settled, report.failed), (MANY as u64, 0));
        assert_eq!(staging.submissions(), 1);
        assert_eq!(staging.remote.registrations.load(Ordering::SeqCst), MANY);
        assert!(staging.owed_docs(&alice()).await.is_empty());
    })
    .unwrap()
}

#[test]
fn each_creator_of_one_transaction_gets_a_list_of_its_own() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let handle = staging.begin().await;
        let mut by_alice = Vec::new();
        let mut by_bob = Vec::new();
        for n in 0..3 {
            by_alice.push(staging.create_in(alice(), &format!("a {n}"), &handle).await);
            if n < 2 {
                by_bob.push(staging.create_in(bob(), &format!("b {n}"), &handle).await);
            }
        }
        staging.commit(&handle).await;

        assert_eq!(staging.submissions(), 2);
        assert_eq!(
            staging.remote.lists(),
            vec![by_alice.clone(), by_bob.clone()]
        );
        for doc in &by_alice {
            assert_eq!(staging.remote.owner(doc), Some(alice()));
        }
        for doc in &by_bob {
            assert_eq!(staging.remote.owner(doc), Some(bob()));
        }
    })
    .unwrap()
}

#[test]
fn an_unreachable_provider_keeps_every_registration_owed() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let handle = staging.begin().await;
        let mut docs = Vec::new();
        for n in 0..MANY {
            docs.push(
                staging
                    .create_in(alice(), &format!("doc {n}"), &handle)
                    .await,
            );
        }
        staging.remote.down.store(true, Ordering::SeqCst);
        let _ = staging.node.runner.commit_txn(&handle).await;

        let owed = staging.owed_docs(&alice()).await;
        assert_eq!(owed.len(), MANY);
        assert!(owed.iter().all(|(_, state)| *state == OwedState::Staged));
        assert_eq!(
            staging.submissions(),
            0,
            "nothing is sent to a provider found away"
        );

        staging.remote.down.store(false, Ordering::SeqCst);
        let report = staging.gate.redrive_owed().await.unwrap();
        assert_eq!((report.settled, report.failed), (MANY as u64, 0));
        for doc in &docs {
            assert_eq!(staging.remote.owner(doc), Some(alice()));
        }
    })
    .unwrap()
}
