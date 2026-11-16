//! Staging relationship changes for an external ACP provider: a transaction
//! records the change it owes instead of failing, with the authority to
//! submit it later, after an advisory check of its acting DID; the count of
//! what it staged follows its statements; and it sees its own staged
//! revocation.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use acp::outbox::{StagedCount, MAX_OWED_CHANGES_PER_TRANSACTION};
use acp::{
    DocumentACP, DocumentPermission, Identity, OwedAuthority, OwedChange, OwedOp, OwedState,
    OwedSubject, StageRelationship, Staged, Subject,
};
use datastore::BasicTxn;
use defra_core::RequestContext;
use identity::Did;
use query::txn::{txn_job, TransactionRegistry};
use query::{QueryExecutor, TransactionHandle};
use storage::RegolithStore;

use super::external_outbox::Gate;
use super::external_registration::{
    alice, bob, gate, mallory, node, request, Node, RemoteAcp, MANAGER, POLICY, RESOURCE,
};

pub(super) fn carol() -> Did {
    Did::new("did:key:z6MkcaroL").unwrap()
}

/// One relationship change to stage.
#[derive(Clone)]
pub(super) struct Change {
    pub(super) actor: Did,
    pub(super) doc: String,
    pub(super) relation: &'static str,
    pub(super) subject: Did,
    pub(super) op: OwedOp,
    /// Whether the policy names a relation that manages `relation`.
    pub(super) managed: bool,
}

impl Change {
    pub(super) fn new(actor: Did, doc: &str, op: OwedOp, subject: Did) -> Self {
        Self {
            actor,
            doc: doc.to_string(),
            relation: "reader",
            subject,
            op,
            managed: true,
        }
    }
}

/// A node on a provider that decides authority itself.
pub(super) struct Staging {
    pub(super) store: Arc<RegolithStore>,
    pub(super) remote: Arc<RemoteAcp>,
    pub(super) gate: Gate,
    pub(super) node: Node,
}

impl Staging {
    pub(super) async fn new() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        Self::on(store, RemoteAcp::new(), true).await
    }

    pub(super) async fn on(
        store: Arc<RegolithStore>,
        remote: Arc<RemoteAcp>,
        create_schema: bool,
    ) -> Self {
        remote.strict.store(true, Ordering::SeqCst);
        let gate = gate(&remote, store.clone());
        let node = node(store.clone(), gate.clone(), create_schema).await;
        Self {
            store,
            remote,
            gate,
            node,
        }
    }

    /// Create a document as `owner`, on its own or in `handle`'s transaction.
    pub(super) async fn create(
        &self,
        owner: Did,
        name: &str,
        handle: Option<&TransactionHandle>,
    ) -> String {
        let mutation = request(
            &format!(r#"mutation {{ add_Users(input: {{name: "{name}"}}) {{ _docID }} }}"#),
            Some(owner),
        );
        let created = match handle {
            Some(handle) => self.node.runner.execute_in_txn(mutation, handle).await,
            None => self.node.runner.execute(mutation).await,
        };
        assert!(!created.has_errors(), "{:?}", created.errors);
        created.data.unwrap()["add_Users"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string()
    }

    pub(super) async fn begin(&self) -> TransactionHandle {
        self.node.runner.begin_txn(false).await.unwrap()
    }

    pub(super) async fn commit(&self, handle: &TransactionHandle) {
        self.node.runner.commit_txn(handle).await.unwrap();
    }

    /// Stage `changes` as one statement of `handle`'s transaction, kept or
    /// undone.
    pub(super) async fn statement(
        &self,
        handle: &TransactionHandle,
        changes: Vec<Change>,
        keep: bool,
    ) -> Vec<acp::Result<Staged>> {
        let gate: Arc<dyn DocumentACP> = self.gate.clone();
        let job = txn_job(move |turn| {
            Box::pin(async move {
                turn.begin_statement().unwrap();
                let results = {
                    let ctx = turn.context().unwrap();
                    let outbox = ctx
                        .doc_fetcher()
                        .transaction_outbox(gate)
                        .await
                        .unwrap()
                        .expect("an external provider keeps an outbox");
                    let managers = [MANAGER.to_string()];
                    let mut results = Vec::new();
                    for change in &changes {
                        results.push(
                            outbox
                                .stage(StageRelationship {
                                    txn: None,
                                    actor: &change.actor,
                                    subject: Subject::Entity(change.subject.clone()),
                                    policy_id: POLICY,
                                    resource: RESOURCE,
                                    object: &change.doc,
                                    relation: change.relation,
                                    managing_relations: if change.managed {
                                        &managers
                                    } else {
                                        &[]
                                    },
                                    op: change.op,
                                })
                                .await,
                        );
                    }
                    results
                };
                turn.end_statement(keep).unwrap();
                results
            })
        });
        self.node
            .registry
            .run_as(&RequestContext::default(), handle, None, job)
            .await
            .unwrap()
    }

    pub(super) async fn stage(
        &self,
        handle: &TransactionHandle,
        change: Change,
    ) -> acp::Result<Staged> {
        self.statement(handle, vec![change], true)
            .await
            .pop()
            .unwrap()
    }

    /// Stage `change` in a transaction of its own and commit it.
    pub(super) async fn commit_change(&self, change: Change) -> Staged {
        let handle = self.begin().await;
        let staged = self.stage(&handle, change).await.unwrap();
        self.commit(&handle).await;
        staged
    }

    /// The committed queue, in sequence order.
    pub(super) async fn queue(&self) -> Vec<OwedChange> {
        let txn = BasicTxn::new(self.store.as_ref(), 0, true).await.unwrap();
        let changes = self.gate.owed_changes(&txn.acpstore(), StagedCount::new());
        let mut queue: Vec<OwedChange> = Vec::new();
        loop {
            let page = changes
                .page(queue.last().map(|change| change.seq))
                .await
                .unwrap();
            if page.is_empty() {
                break;
            }
            queue.extend(page);
        }
        drop(changes);
        txn.discard().unwrap();
        queue
    }

    /// The names `identity` reads through `handle`'s transaction.
    pub(super) async fn reads_in(&self, handle: &TransactionHandle, identity: Did) -> Vec<String> {
        let response = self
            .node
            .runner
            .execute_in_txn(request("{ Users { name } }", Some(identity)), handle)
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        response.data.unwrap()["Users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|user| user["name"].as_str().unwrap().to_string())
            .collect()
    }

    pub(super) async fn allowed(&self, identity: &Did, doc: &str) -> bool {
        self.gate
            .check_doc_access(
                &Identity::Authenticated(identity.clone()),
                DocumentPermission::Read,
                POLICY,
                RESOURCE,
                doc,
            )
            .await
            .unwrap()
    }

    /// `requestor` changes a relationship on the provider directly.
    pub(super) async fn on_provider(&self, requestor: Did, change: Change) {
        let call = if change.op == OwedOp::Add {
            self.remote
                .add_actor_relationship(
                    &requestor,
                    &change.subject,
                    POLICY,
                    RESOURCE,
                    &change.doc,
                    change.relation,
                    &[],
                )
                .await
        } else {
            self.remote
                .delete_actor_relationship(
                    &requestor,
                    &change.subject,
                    POLICY,
                    RESOURCE,
                    &change.doc,
                    change.relation,
                    &[],
                )
                .await
        };
        call.unwrap();
        self.remote.take_writes();
    }
}

#[test]
fn only_a_provider_that_cannot_join_a_transaction_keeps_an_outbox() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let local = acp::ZanzibarDocumentACP::new(Arc::new(
            acp::PersistentZanzibarStore::from_store(staging.store.clone()),
        ));
        let txn = staging.node.db.new_txn(false).await.unwrap();
        let view = txn.acpstore().unwrap();

        assert!(local.supports_transactions());
        assert!(local.transaction_outbox(&view, txn.owed_staged()).is_none());
        assert!(local.bind_transaction(view.clone()).is_some());

        assert!(!staging.gate.supports_transactions());
        assert!(staging.gate.bind_transaction(view.clone()).is_none());
        assert!(staging
            .gate
            .transaction_outbox(&view, txn.owed_staged())
            .is_some());
        assert!(staging
            .remote
            .transaction_outbox(&view, txn.owed_staged())
            .is_none());
        drop(view);
        txn.discard().unwrap();
    })
    .unwrap()
}

#[test]
fn an_owner_stages_a_change_that_commits_or_rolls_back_with_its_transaction() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;

        let discarded = staging.begin().await;
        let staged = staging
            .stage(&discarded, Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await
            .unwrap();
        assert!(staged.authority_checked);
        staging.node.runner.discard_txn(&discarded).await.unwrap();
        assert!(staging.queue().await.is_empty());

        let handle = staging.begin().await;
        let staged = staging
            .stage(&handle, Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await
            .unwrap();
        assert!(staging.queue().await.is_empty(), "owed before its commit");
        assert!(staging.remote.take_writes().is_empty());
        staging.commit(&handle).await;
        assert!(staging.remote.take_writes().is_empty());

        let queue = staging.queue().await;
        assert_eq!(queue.len(), 1);
        let change = &queue[0];
        assert_eq!(change.seq, staged.seq);
        assert_eq!(change.op, OwedOp::Add);
        assert_eq!(change.actor, alice());
        assert_eq!(change.tuple.object, doc);
        assert_eq!(change.tuple.subject, OwedSubject::Actor { did: bob() });
        assert_eq!(change.authority, OwedAuthority::NodeKey);
        assert_eq!(change.state, OwedState::Staged);
        assert_eq!(change.attempts, 0);
        assert!(
            !staging.allowed(&bob(), &doc).await,
            "an owed add grants nothing"
        );
    })
    .unwrap()
}

#[test]
fn staging_refuses_whoever_neither_owns_nor_manages_as_a_local_provider_does() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let mut manager = Change::new(alice(), &doc, OwedOp::Add, bob());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager).await;

        let handle = staging.begin().await;
        let by_mallory = Change::new(mallory(), &doc, OwedOp::Add, carol());
        assert!(matches!(
            staging.stage(&handle, by_mallory.clone()).await,
            Err(acp::Error::NotManager { .. })
        ));
        let unmanaged = Change {
            managed: false,
            ..by_mallory
        };
        assert!(matches!(
            staging.stage(&handle, unmanaged).await,
            Err(acp::Error::NotOwner { .. })
        ));
        let unregistered = Change::new(alice(), "bae-unknown", OwedOp::Add, carol());
        assert!(matches!(
            staging.stage(&handle, unregistered).await,
            Err(acp::Error::NotManager { .. })
        ));

        let by_manager = staging
            .stage(&handle, Change::new(bob(), &doc, OwedOp::Delete, carol()))
            .await
            .unwrap();
        assert!(by_manager.authority_checked);
        staging.commit(&handle).await;
        let queue = staging.queue().await;
        assert_eq!(queue.len(), 1, "a refused change is not staged");
        assert_eq!(queue[0].actor, bob());
    })
    .unwrap()
}

#[test]
fn a_creator_grants_on_a_document_created_in_the_same_transaction() {
    workers::block_on(async {
        let staging = Staging::new().await;
        // The provider is unreachable: only the transaction's own owed
        // registration can say who created the document.
        staging.remote.down.store(true, Ordering::SeqCst);
        let handle = staging.begin().await;
        let doc = staging.create(alice(), "fresh", Some(&handle)).await;

        let granted = staging
            .stage(&handle, Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await
            .unwrap();
        assert!(granted.authority_checked);
        assert!(matches!(
            staging
                .stage(
                    &handle,
                    Change::new(mallory(), &doc, OwedOp::Add, mallory())
                )
                .await,
            Err(acp::Error::NotManager { .. })
        ));
        assert_eq!(staging.remote.owner_lookups.load(Ordering::SeqCst), 0);
        let _ = staging.node.runner.commit_txn(&handle).await;
        assert_eq!(staging.queue().await.len(), 1);
    })
    .unwrap()
}

#[test]
fn a_provider_that_cannot_answer_leaves_the_change_staged_and_unchecked() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.remote.down.store(true, Ordering::SeqCst);

        let handle = staging.begin().await;
        // Mallory has no authority, and nobody can say so now.
        let staged = staging
            .stage(
                &handle,
                Change::new(mallory(), &doc, OwedOp::Add, mallory()),
            )
            .await
            .unwrap();
        assert!(!staged.authority_checked);
        staging.commit(&handle).await;
        assert_eq!(staging.queue().await.len(), 1);
    })
    .unwrap()
}

#[test]
fn the_bound_of_one_transaction_follows_its_statements() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let changes = |count: u32| {
            (0..count)
                .map(|_| Change::new(alice(), &doc, OwedOp::Add, bob()))
                .collect::<Vec<_>>()
        };
        let all_staged = |results: Vec<acp::Result<Staged>>| results.iter().all(Result::is_ok);
        let limit = MAX_OWED_CHANGES_PER_TRANSACTION;

        let handle = staging.begin().await;
        assert!(all_staged(
            staging.statement(&handle, changes(limit - 56), true).await
        ));
        // A statement that is undone gives back what it staged.
        assert!(all_staged(
            staging.statement(&handle, changes(56), false).await
        ));
        assert!(all_staged(
            staging.statement(&handle, changes(56), true).await
        ));
        let over = staging.statement(&handle, changes(1), true).await;
        assert!(matches!(
            over[0],
            Err(acp::Error::OwedChangesPerTransaction { limit: bound }) if bound == limit
        ));
        // A statement that fails at the bound keeps what the others staged.
        let undone = staging.statement(&handle, changes(1), false).await;
        assert!(matches!(
            undone[0],
            Err(acp::Error::OwedChangesPerTransaction { .. })
        ));
        staging.commit(&handle).await;
        assert_eq!(staging.queue().await.len(), limit as usize);

        let next = staging.begin().await;
        assert!(staging
            .stage(&next, Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await
            .is_ok());
        staging.node.runner.discard_txn(&next).await.unwrap();
    })
    .unwrap()
}

#[test]
fn a_change_records_the_nodes_key_or_the_requests_delegation_or_is_not_staged() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.remote.credentials.store(true, Ordering::SeqCst);
        let change = || Change::new(alice(), &doc, OwedOp::Add, bob());
        let handle = staging.begin().await;

        assert!(matches!(
            staging.stage(&handle, change()).await,
            Err(acp::Error::OwedAuthorityUnavailable { actor, .. }) if actor == alice().as_str()
        ));

        staging
            .remote
            .carry_delegation(&alice(), "alice.delegation");
        let delegated = staging.stage(&handle, change()).await.unwrap();
        // A key the node holds is used before a delegation is recorded.
        staging.remote.hold_key(&alice());
        let keyed = staging.stage(&handle, change()).await.unwrap();

        staging.commit(&handle).await;
        let queue = staging.queue().await;
        assert_eq!(queue.len(), 2, "a change with no authority is not staged");
        assert_eq!(queue[0].seq, delegated.seq);
        assert_eq!(
            queue[0].authority,
            OwedAuthority::Delegation {
                token: "alice.delegation".into()
            }
        );
        assert_eq!(queue[1].seq, keyed.seq);
        assert_eq!(queue[1].authority, OwedAuthority::NodeKey);
    })
    .unwrap()
}

#[test]
fn a_revocation_under_one_spelling_denies_a_request_under_another() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let other_bob = Did::new("did:key:z6MkboB-other-spelling").unwrap();
        let other_alice = Did::new("did:key:z6MkalicE-other-spelling").unwrap();
        staging.remote.respell(&other_bob, &bob());
        staging.remote.respell(&other_alice, &alice());
        staging
            .on_provider(alice(), Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        staging
            .on_provider(
                alice(),
                Change::new(alice(), &doc, OwedOp::Add, other_bob.clone()),
            )
            .await;
        assert!(staging.allowed(&bob(), &doc).await);
        assert!(staging.allowed(&other_bob, &doc).await);

        let handle = staging.begin().await;
        let staged = staging
            .stage(
                &handle,
                Change::new(other_alice, &doc, OwedOp::Delete, other_bob.clone()),
            )
            .await
            .unwrap();
        assert!(staged.authority_checked, "the owner under another spelling");
        staging.commit(&handle).await;

        let queue = staging.queue().await;
        assert_eq!(
            queue[0].actor,
            alice(),
            "the acting DID is kept in one spelling"
        );
        assert_eq!(queue[0].tuple.subject, OwedSubject::Actor { did: bob() });
        let asked = staging.remote.access_checks.load(Ordering::SeqCst);
        assert!(!staging.allowed(&bob(), &doc).await);
        assert!(!staging.allowed(&other_bob, &doc).await);
        assert_eq!(
            staging.remote.access_checks.load(Ordering::SeqCst),
            asked,
            "the gate denies both spellings before the provider is asked"
        );
        assert!(staging.allowed(&alice(), &doc).await);
    })
    .unwrap()
}

#[test]
fn a_transaction_sees_its_own_staged_revocation_and_not_its_staged_grant() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        for reader in [bob(), mallory()] {
            staging
                .on_provider(alice(), Change::new(alice(), &doc, OwedOp::Add, reader))
                .await;
        }

        let handle = staging.begin().await;
        assert_eq!(staging.reads_in(&handle, bob()).await, ["shared"]);

        // A statement that is undone revokes nothing.
        staging
            .statement(
                &handle,
                vec![Change::new(alice(), &doc, OwedOp::Delete, bob())],
                false,
            )
            .await;
        assert_eq!(staging.reads_in(&handle, bob()).await, ["shared"]);

        staging
            .stage(&handle, Change::new(alice(), &doc, OwedOp::Delete, bob()))
            .await
            .unwrap();
        staging
            .stage(&handle, Change::new(alice(), &doc, OwedOp::Add, carol()))
            .await
            .unwrap();
        assert!(staging.reads_in(&handle, bob()).await.is_empty());
        assert!(
            staging.reads_in(&handle, carol()).await.is_empty(),
            "a staged grant is not honoured"
        );
        assert_eq!(staging.reads_in(&handle, mallory()).await, ["shared"]);
        assert_eq!(staging.reads_in(&handle, alice()).await, ["shared"]);
        // The revoked subject is refused the authority it would manage with.
        assert!(matches!(
            staging
                .stage(&handle, Change::new(bob(), &doc, OwedOp::Add, bob()))
                .await,
            Err(acp::Error::PermissionDenied(_))
        ));
        // Nothing is committed: every other reader still sees the grant.
        assert!(staging.allowed(&bob(), &doc).await);

        staging.node.runner.discard_txn(&handle).await.unwrap();
        assert!(staging.allowed(&bob(), &doc).await);
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_staged_revocation_of_everyone_spares_the_owner_in_its_transaction() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(
                alice(),
                Change::new(alice(), &doc, OwedOp::Add, Did::wildcard()),
            )
            .await;

        let handle = staging.begin().await;
        assert_eq!(staging.reads_in(&handle, bob()).await, ["shared"]);
        staging
            .stage(
                &handle,
                Change::new(alice(), &doc, OwedOp::Delete, Did::wildcard()),
            )
            .await
            .unwrap();
        assert!(staging.reads_in(&handle, bob()).await.is_empty());
        assert_eq!(staging.reads_in(&handle, alice()).await, ["shared"]);
        staging.node.runner.discard_txn(&handle).await.unwrap();
    })
    .unwrap()
}

/// A write decides inside an engine attempt, where the provider is not asked:
/// the owner an owed revocation spares comes from the request's own inputs.
#[test]
fn a_committed_revocation_denies_a_write_and_spares_the_owner() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(
                alice(),
                Change::new(alice(), &doc, OwedOp::Add, Did::wildcard()),
            )
            .await;
        let rename = |identity: Did, name: &'static str| {
            let (staging, doc) = (&staging, &doc);
            async move {
                let response = staging
                    .node
                    .runner
                    .execute(request(
                        &format!(
                            r#"mutation {{ update_Users(docID: "{doc}", input: {{name: "{name}"}}) {{ name }} }}"#
                        ),
                        Some(identity),
                    ))
                    .await;
                response
                    .data
                    .filter(|_| response.errors.is_empty())
                    .and_then(|data| data["update_Users"].as_array().map(Vec::len))
                    .unwrap_or(0)
            }
        };
        assert_eq!(rename(bob(), "by bob").await, 1);

        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Delete, Did::wildcard()))
            .await;
        assert_eq!(rename(bob(), "by bob again").await, 0);
        assert_eq!(rename(alice(), "by alice").await, 1);
        assert_eq!(staging.reads_in(&staging.begin().await, alice()).await, ["by alice"]);
    })
    .unwrap()
}
