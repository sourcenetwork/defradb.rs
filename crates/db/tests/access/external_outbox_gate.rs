//! The gate on relationship changes owed to an external ACP provider: while
//! any delete of a tuple is unsettled, its subject is denied on its object
//! before the provider is asked, whatever was staged after the delete; an owed
//! add grants nothing; and with nothing owed the provider is asked exactly as
//! before.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use acp::outbox::StagedCount;
use acp::{DocumentACP, DocumentPermission, Identity, OwedChanges, OwedOp, OwedSubject, OwedTuple};
use identity::Did;
use query::QueryExecutor;
use storage::RegolithStore;

use super::external_outbox::{actor, owed, tuple, Gate};
use super::external_registration::{
    alice, bob, gate, mallory, node, readable, request, Node, RemoteAcp, POLICY, RESOURCE,
};

/// A node whose provider holds one document of Alice's.
struct Shared {
    remote: Arc<RemoteAcp>,
    gate: Gate,
    node: Node,
    doc_id: String,
}

impl Shared {
    async fn new() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let remote = RemoteAcp::new();
        let gate = gate(&remote, store.clone());
        let node = node(store, gate.clone(), true).await;
        let created = node
            .runner
            .execute(request(
                r#"mutation { add_Users(input: {name: "shared"}) { _docID } }"#,
                Some(alice()),
            ))
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc_id = created.data.unwrap()["add_Users"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(remote.owner(&doc_id), Some(alice()));
        Self {
            remote,
            gate,
            node,
            doc_id,
        }
    }

    /// `requestor` shares the document with `target` through the gate.
    async fn share(&self, requestor: Did, target: &Did, relation: &str) -> acp::Result<bool> {
        self.gate
            .add_actor_relationship(
                &requestor,
                target,
                POLICY,
                RESOURCE,
                &self.doc_id,
                relation,
                &[],
            )
            .await
    }

    async fn grant(&self, target: &Did) {
        assert!(self.share(alice(), target, "reader").await.unwrap());
    }

    fn reader(&self, subject: OwedSubject) -> OwedTuple {
        tuple(&self.doc_id, "reader", subject)
    }

    async fn begin(&self) -> (db::DbTxn<RegolithStore>, OwedChanges) {
        let txn = self.node.db.new_txn(false).await.unwrap();
        let changes = self
            .gate
            .owed_changes(&txn.acpstore().unwrap(), StagedCount::new());
        (txn, changes)
    }

    async fn commit(&self, tuple: &OwedTuple, op: OwedOp) -> u64 {
        let (txn, changes) = self.begin().await;
        let seq = changes.stage(owed(tuple, op)).await.unwrap().seq;
        txn.commit().await.unwrap();
        seq
    }

    async fn settle(&self, seq: u64) {
        let (txn, changes) = self.begin().await;
        assert!(changes.settle(seq).await.unwrap());
        txn.commit().await.unwrap();
    }

    async fn reads(&self, identity: Option<Did>) -> bool {
        !readable(&self.node.runner, identity).await.is_empty()
    }

    async fn allowed(&self, identity: &Identity) -> bool {
        self.gate
            .check_doc_access(
                identity,
                DocumentPermission::Read,
                POLICY,
                RESOURCE,
                &self.doc_id,
            )
            .await
            .unwrap()
    }
}

#[test]
fn an_owed_delete_denies_its_subject_from_its_commit_until_it_settles() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&bob()).await;
        shared.grant(&mallory()).await;
        assert!(shared.reads(Some(bob())).await);

        let reader = shared.reader(actor(bob()));
        let (txn, changes) = shared.begin().await;
        let seq = changes
            .stage(owed(&reader, OwedOp::Delete))
            .await
            .unwrap()
            .seq;
        assert!(
            shared.reads(Some(bob())).await,
            "an uncommitted delete denies no one"
        );
        txn.commit().await.unwrap();

        assert!(!shared.reads(Some(bob())).await);
        assert!(!shared.allowed(&Identity::Authenticated(bob())).await);
        assert!(shared.reads(Some(mallory())).await, "another reader stays");
        assert!(shared.reads(Some(alice())).await);

        shared.settle(seq).await;
        assert!(
            shared.reads(Some(bob())).await,
            "once settled, the provider decides alone"
        );
    })
    .unwrap()
}

#[test]
fn a_rolled_back_delete_denies_no_one() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&bob()).await;
        let (txn, changes) = shared.begin().await;
        changes
            .stage(owed(&shared.reader(actor(bob())), OwedOp::Delete))
            .await
            .unwrap();
        txn.discard().unwrap();
        assert!(shared.reads(Some(bob())).await);
    })
    .unwrap()
}

#[test]
fn a_later_owed_add_of_the_tuple_does_not_lift_the_denial() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&bob()).await;
        let reader = shared.reader(actor(bob()));

        let delete = shared.commit(&reader, OwedOp::Delete).await;
        assert!(!shared.reads(Some(bob())).await);
        let add = shared.commit(&reader, OwedOp::Add).await;
        assert!(
            !shared.reads(Some(bob())).await,
            "the provider may refuse the add and has not seen the delete"
        );
        let again = shared.commit(&reader, OwedOp::Delete).await;

        shared.settle(add).await;
        assert!(!shared.reads(Some(bob())).await);
        shared.settle(delete).await;
        assert!(
            !shared.reads(Some(bob())).await,
            "one delete of the tuple is still unsettled"
        );
        shared.settle(again).await;
        assert!(
            shared.reads(Some(bob())).await,
            "every delete of the tuple is settled"
        );
    })
    .unwrap()
}

#[test]
fn an_owed_add_grants_nothing() {
    workers::block_on(async {
        let shared = Shared::new().await;
        let stranger = shared.reader(actor(mallory()));
        shared.commit(&stranger, OwedOp::Add).await;
        assert!(
            !shared.reads(Some(mallory())).await,
            "an owed add waits for the provider"
        );
    })
    .unwrap()
}

#[test]
fn an_owed_wildcard_delete_denies_everyone_but_the_owner() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&Did::wildcard()).await;
        shared.grant(&bob()).await;
        assert!(shared.reads(Some(mallory())).await);
        assert!(shared.reads(None).await);

        let seq = shared
            .commit(&shared.reader(OwedSubject::Wildcard), OwedOp::Delete)
            .await;
        assert!(!shared.reads(Some(mallory())).await);
        assert!(!shared.reads(None).await);
        assert!(
            !shared.reads(Some(bob())).await,
            "a reader by another path waits for the settlement too"
        );
        assert!(shared.reads(Some(alice())).await);
        assert!(shared.allowed(&Identity::Authenticated(alice())).await);

        shared.settle(seq).await;
        assert!(shared.reads(Some(mallory())).await);
        assert!(shared.reads(None).await);
    })
    .unwrap()
}

#[test]
fn an_owed_delete_of_a_set_denies_everyone_but_the_owner() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&bob()).await;
        let members = shared.reader(OwedSubject::EntitySet {
            resource: "groups".into(),
            object: "team".into(),
            relation: "member".into(),
        });
        shared.commit(&members, OwedOp::Delete).await;
        assert!(!shared.reads(Some(bob())).await);
        assert!(shared.reads(Some(alice())).await);
    })
    .unwrap()
}

#[test]
fn the_owner_is_never_denied_by_a_delete_that_names_it() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&alice()).await;
        shared
            .commit(&shared.reader(actor(alice())), OwedOp::Delete)
            .await;
        assert!(shared.reads(Some(alice())).await);
        assert!(shared.allowed(&Identity::Authenticated(alice())).await);
        assert!(shared.share(alice(), &mallory(), "reader").await.unwrap());
    })
    .unwrap()
}

#[test]
fn a_delete_on_one_object_denies_nothing_on_another() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&bob()).await;
        let elsewhere = tuple("another-document", "reader", actor(bob()));
        shared.commit(&elsewhere, OwedOp::Delete).await;
        shared
            .commit(
                &tuple("another-document", "reader", OwedSubject::Wildcard),
                OwedOp::Delete,
            )
            .await;
        assert!(shared.reads(Some(bob())).await);
    })
    .unwrap()
}

#[test]
fn an_owed_delete_takes_decisions_and_relationship_authority_from_its_subject() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&bob()).await;
        let decision = |identity: Identity| {
            let shared = &shared;
            async move {
                shared
                    .gate
                    .create_access_decision(&identity, POLICY, RESOURCE, &shared.doc_id, "read")
                    .await
            }
        };
        assert_eq!(
            decision(Identity::Authenticated(bob())).await.unwrap(),
            None
        );
        assert!(shared.share(bob(), &mallory(), "reader").await.unwrap());

        let seq = shared
            .commit(&shared.reader(actor(bob())), OwedOp::Delete)
            .await;
        assert!(matches!(
            decision(Identity::Authenticated(bob())).await,
            Err(acp::Error::PermissionDenied(_))
        ));
        assert_eq!(
            decision(Identity::Authenticated(alice())).await.unwrap(),
            None
        );
        for relation in ["reader", "writer"] {
            assert!(matches!(
                shared.share(bob(), &mallory(), relation).await,
                Err(acp::Error::PermissionDenied(_))
            ));
        }
        let revoke = shared
            .gate
            .delete_actor_relationship(
                &bob(),
                &mallory(),
                POLICY,
                RESOURCE,
                &shared.doc_id,
                "reader",
                &[],
            )
            .await;
        assert!(matches!(revoke, Err(acp::Error::PermissionDenied(_))));
        assert!(
            shared.reads(Some(mallory())).await,
            "the refused revocation removed nothing"
        );

        shared.settle(seq).await;
        assert_eq!(
            decision(Identity::Authenticated(bob())).await.unwrap(),
            None
        );
    })
    .unwrap()
}

#[test]
fn with_nothing_owed_the_provider_is_asked_exactly_as_before() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared.grant(&bob()).await;
        let checks = || shared.remote.access_checks.load(Ordering::SeqCst);
        let lookups = || shared.remote.owner_lookups.load(Ordering::SeqCst);

        let (checks_before, lookups_before) = (checks(), lookups());
        for identity in [
            Identity::Authenticated(bob()),
            Identity::Authenticated(mallory()),
            Identity::Anonymous,
        ] {
            let direct = shared
                .remote
                .check_doc_access(
                    &identity,
                    DocumentPermission::Read,
                    POLICY,
                    RESOURCE,
                    &shared.doc_id,
                )
                .await
                .unwrap();
            assert_eq!(shared.allowed(&identity).await, direct);
        }
        assert_eq!(checks() - checks_before, 6, "one provider check per call");
        assert_eq!(lookups(), lookups_before, "the owner is not looked up");

        let add = shared
            .commit(&shared.reader(actor(mallory())), OwedOp::Add)
            .await;
        let checks_before = checks();
        assert!(shared.allowed(&Identity::Authenticated(bob())).await);
        assert_eq!(checks() - checks_before, 1);
        assert_eq!(
            lookups(),
            lookups_before,
            "an owed add asks nothing more of the provider"
        );
        shared.settle(add).await;

        shared
            .commit(&shared.reader(actor(bob())), OwedOp::Delete)
            .await;
        let checks_before = checks();
        assert!(!shared.allowed(&Identity::Authenticated(bob())).await);
        assert_eq!(
            checks(),
            checks_before,
            "the denial is decided before the provider is asked"
        );
    })
    .unwrap()
}

#[test]
fn an_unreachable_provider_denies_whoever_might_be_the_owner() {
    workers::block_on(async {
        let shared = Shared::new().await;
        shared
            .commit(&shared.reader(OwedSubject::Wildcard), OwedOp::Delete)
            .await;
        shared.remote.down.store(true, Ordering::SeqCst);
        let alice = Identity::Authenticated(alice());
        let asked = shared
            .gate
            .check_doc_access(
                &alice,
                DocumentPermission::Read,
                POLICY,
                RESOURCE,
                &shared.doc_id,
            )
            .await;
        assert!(asked.is_err(), "no answer is never read as access");
    })
    .unwrap()
}
