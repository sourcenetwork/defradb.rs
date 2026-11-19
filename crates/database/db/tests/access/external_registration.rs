//! External (non-transactional) ACP: a create owes its registration through a
//! record written in its own transaction. Until the provider confirms, the
//! document is readable only by its creator; a redrive settles the record
//! after a provider failure or a crash; an upsert that updates an existing
//! document never registers its caller.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity};
use db::{
    AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB,
};
use identity::Did;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use regolith::sync::{HopscotchMap, SegQueue};
use schema::PolicyDescription;
use storage::RegolithStore;

pub(super) const POLICY: &str = "remote-policy";
pub(super) const RESOURCE: &str = "users";

type Set<K> = HopscotchMap<K, (), rapidhash::fast::RandomState>;

fn set<K>() -> Set<K>
where
    K: std::hash::Hash + Eq + Clone + 'static,
{
    HopscotchMap::with_hasher(rapidhash::fast::RandomState::default())
}

/// The relation whose holders manage the others on [`RemoteAcp`].
pub(super) const MANAGER: &str = "manager";

/// A remote provider: unregistered documents are public, a registered one is
/// readable by its owner and by whoever holds a relationship on it, and every
/// call fails while it is down.
///
/// It can be told to behave as a provider that decides authority itself
/// (`strict`), that takes its callers' credentials (`credentials`), and that
/// applies a write and loses the answer (`lose_answer`).
pub(super) struct RemoteAcp {
    owners: HopscotchMap<String, Did, rapidhash::fast::RandomState>,
    grants: Set<(String, String)>,
    managers: Set<(String, String)>,
    pub(super) down: AtomicBool,
    pub(super) registrations: AtomicUsize,
    pub(super) access_checks: AtomicUsize,
    pub(super) owner_lookups: AtomicUsize,
    /// A relationship write is refused unless its requestor owns the
    /// document or holds [`MANAGER`] on it.
    pub(super) strict: AtomicBool,
    /// The next relationship write is applied and then fails, as when the
    /// answer is lost on the way back.
    pub(super) lose_answer: AtomicBool,
    /// A registration is refused: its creator's credentials are missing.
    pub(super) refuse_registration: AtomicBool,
    /// A registration is answered with an error that does not say whether
    /// it is final.
    pub(super) garble_registration: AtomicBool,
    /// A relationship write is answered with this error, which does not say
    /// whether it is final, and is not applied.
    garbled: HopscotchMap<(), String, rapidhash::fast::RandomState>,
    /// The owner the provider names is not a DID the node can read.
    pub(super) malformed_owner: AtomicBool,
    /// The node cannot make the request for an owed change at all.
    pub(super) unsendable: AtomicBool,
    /// The actors the node cannot make that request for.
    unsigned: Set<String>,
    /// Every relationship write applied, in order: add or delete, document,
    /// subject.
    writes: SegQueue<(bool, String, String)>,
    /// Authority is decided from `keys` and `delegations` instead of being
    /// the node's for every DID.
    pub(super) credentials: AtomicBool,
    keys: Set<String>,
    delegations: HopscotchMap<String, String, rapidhash::fast::RandomState>,
    /// Delegations an actor bound to one relationship command: actor,
    /// document, subject and whether it deletes.
    bound: HopscotchMap<(String, String, String, bool), String, rapidhash::fast::RandomState>,
    expired: Set<String>,
    /// Other spellings of an actor, by the one the provider keeps.
    spellings: HopscotchMap<String, Did, rapidhash::fast::RandomState>,
    /// The journal owed changes go through, when the provider keeps one.
    pub(super) journal: regolith::sync::OnceCell<Arc<dyn acp::OwedJournal>>,
    /// Run once an exact observation has its answer, before it returns.
    pub(super) observing: HopscotchMap<(), ObserveHook, rapidhash::fast::RandomState>,
}

/// What a test runs inside an exact observation, after the provider answered.
pub(super) type ObserveHook =
    Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>;

impl RemoteAcp {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            owners: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
            grants: set(),
            managers: set(),
            down: AtomicBool::new(false),
            registrations: AtomicUsize::new(0),
            access_checks: AtomicUsize::new(0),
            owner_lookups: AtomicUsize::new(0),
            strict: AtomicBool::new(false),
            lose_answer: AtomicBool::new(false),
            refuse_registration: AtomicBool::new(false),
            garble_registration: AtomicBool::new(false),
            garbled: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
            malformed_owner: AtomicBool::new(false),
            unsendable: AtomicBool::new(false),
            writes: SegQueue::new(),
            credentials: AtomicBool::new(false),
            keys: set(),
            unsigned: set(),
            delegations: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
            bound: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
            expired: set(),
            spellings: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
            journal: regolith::sync::OnceCell::new(),
            observing: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
        })
    }

    /// `did` carries `token`, a delegation bound to one change only: `did`'s
    /// change of `subject` on `doc_id`, an add or a delete.
    pub(super) fn bind_delegation(
        &self,
        did: &Did,
        doc_id: &str,
        subject: &Did,
        delete: bool,
        token: &str,
    ) {
        self.bound.insert(
            (
                did.to_string(),
                doc_id.to_string(),
                subject.to_string(),
                delete,
            ),
            token.to_string(),
        );
    }

    /// Answer relationship writes with `error`, or apply them again.
    pub(super) fn garble(&self, error: Option<&str>) {
        match error {
            Some(error) => self.garbled.insert((), error.to_string()),
            None => self.garbled.remove(&()),
        };
    }

    /// The node holds `did`'s key.
    pub(super) fn hold_key(&self, did: &Did) {
        self.keys.insert(did.to_string(), ());
    }

    /// Whether the node can make the request for an owed change of `did`.
    pub(super) fn can_sign(&self, did: &Did, can: bool) {
        if can {
            self.unsigned.remove(&did.to_string());
        } else {
            self.unsigned.insert(did.to_string(), ());
        }
    }

    pub(super) fn drop_key(&self, did: &Did) {
        self.keys.remove(&did.to_string());
    }

    /// The request being served carries `token` for `did`.
    pub(super) fn carry_delegation(&self, did: &Did, token: &str) {
        self.delegations.insert(did.to_string(), token.to_string());
    }

    /// The request being served carries nothing for `did` any more.
    pub(super) fn forget_delegation(&self, did: &Did) {
        self.delegations.remove(did.as_str());
    }

    pub(super) fn expire(&self, token: &str) {
        self.expired.insert(token.to_string(), ());
    }

    /// `other` is another spelling of the actor the provider keeps as `kept`.
    pub(super) fn respell(&self, other: &Did, kept: &Did) {
        self.spellings.insert(other.to_string(), kept.clone());
    }

    /// The relationship writes applied so far, oldest first, taken out.
    pub(super) fn take_writes(&self) -> Vec<(bool, String, String)> {
        std::iter::from_fn(|| self.writes.pop()).collect()
    }

    pub(super) fn holds(&self, doc_id: &str, subject: &Did) -> bool {
        self.granted(doc_id, subject.as_str())
    }

    pub(super) fn manages(&self, requestor: &Did, doc_id: &str) -> bool {
        self.owners.get(doc_id).as_ref() == Some(requestor)
            || self
                .managers
                .contains_key(&(doc_id.to_string(), requestor.to_string()))
    }

    /// Apply one relationship write as the provider decides it.
    fn write(
        &self,
        add: bool,
        requestor: &Did,
        target: &Did,
        doc_id: &str,
        relation: &str,
    ) -> acp::Result<bool> {
        self.reachable()?;
        if let Some(error) = self.garbled.get(&()) {
            return Err(acp::Error::Storage(error));
        }
        if self.strict.load(Ordering::SeqCst) && !self.manages(requestor, doc_id) {
            return Err(acp::Error::NotManager {
                operation: "change relationship".into(),
            });
        }
        let key = (doc_id.to_string(), target.to_string());
        let held = if relation == MANAGER {
            &self.managers
        } else {
            &self.grants
        };
        let changed = if add {
            held.insert(key, ()).is_none()
        } else {
            held.remove(&key).is_some()
        };
        self.writes
            .push((add, doc_id.to_string(), target.to_string()));
        if self.lose_answer.swap(false, Ordering::SeqCst) {
            return Err(acp::Error::ProviderUnavailable("answer lost".into()));
        }
        Ok(changed)
    }

    fn granted(&self, doc_id: &str, subject: &str) -> bool {
        self.grants
            .contains_key(&(doc_id.to_string(), subject.to_string()))
    }

    fn reachable(&self) -> acp::Result<()> {
        if self.down.load(Ordering::SeqCst) {
            return Err(acp::Error::ProviderUnavailable(
                "provider unreachable".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn owner(&self, doc_id: &str) -> Option<Did> {
        self.owners.get(doc_id)
    }
}

#[async_trait::async_trait]
impl DocumentACP for RemoteAcp {
    async fn register_doc_object(
        &self,
        identity: &Did,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<()> {
        self.reachable()?;
        if self.refuse_registration.load(Ordering::SeqCst) {
            return Err(acp::Error::PermissionDenied(
                "no credentials for the creator".into(),
            ));
        }
        if self.garble_registration.load(Ordering::SeqCst) {
            return Err(acp::Error::Storage("code 13".into()));
        }
        if let Some(owner) = self.owners.get(doc_id) {
            if owner != *identity {
                return Err(acp::Error::DocumentAlreadyRegistered(doc_id.into()));
            }
            return Ok(());
        }
        self.owners.insert(doc_id.to_string(), identity.clone());
        self.registrations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn is_doc_registered(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        self.reachable()?;
        Ok(self.owners.contains_key(doc_id))
    }

    async fn get_doc_owner(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<Option<Did>> {
        self.reachable()?;
        self.owner_lookups.fetch_add(1, Ordering::SeqCst);
        if self.malformed_owner.load(Ordering::SeqCst) {
            return Err(acp::Error::Storage("invalid owner DID".into()));
        }
        Ok(self.owners.get(doc_id))
    }

    async fn check_doc_access(
        &self,
        identity: &Identity,
        _permission: DocumentPermission,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        self.reachable()?;
        self.access_checks.fetch_add(1, Ordering::SeqCst);
        Ok(match (self.owners.get(doc_id), identity) {
            (None, _) => true,
            (Some(_), _) if self.granted(doc_id, "*") => true,
            (Some(owner), Identity::Authenticated(did)) => {
                *did == owner || self.granted(doc_id, did.as_str())
            }
            (Some(_), _) => false,
        })
    }

    async fn add_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        _policy_id: &str,
        _collection_id: &str,
        doc_id: &str,
        relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        self.write(true, requestor, target, doc_id, relation)
    }

    async fn delete_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        _policy_id: &str,
        _collection_id: &str,
        doc_id: &str,
        relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        self.write(false, requestor, target, doc_id, relation)
    }

    async fn check_named_permission(
        &self,
        subject: &Did,
        permission: &str,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        self.reachable()?;
        Ok(permission == MANAGER
            && self
                .managers
                .contains_key(&(doc_id.to_string(), subject.to_string())))
    }

    /// A requester who neither owns the document nor manages its relations
    /// is refused alike, registered or not, as vera.rs refuses one.
    async fn observe_actor_relationship(
        &self,
        observation: acp::ActorRelationshipObservation<'_>,
    ) -> acp::Result<bool> {
        self.reachable()?;
        if !self.manages(observation.requester, observation.document) {
            return Err(acp::Error::NotManager {
                operation: "observe relationship".into(),
            });
        }
        let key = (
            observation.document.to_string(),
            observation.target.to_string(),
        );
        let held = if observation.relation == MANAGER {
            self.managers.contains_key(&key)
        } else {
            self.grants.contains_key(&key)
        };
        if let Some(hook) = self.observing.get(&()) {
            hook().await;
        }
        Ok(held)
    }

    fn canonical_actor(&self, did: &Did) -> Did {
        self.spellings
            .get(did.as_str())
            .unwrap_or_else(|| did.clone())
    }

    fn owed_authority(&self, actor: &Did) -> acp::Result<acp::OwedAuthority> {
        if !self.credentials.load(Ordering::SeqCst) || self.keys.contains_key(actor.as_str()) {
            return Ok(acp::OwedAuthority::NodeKey);
        }
        match self.delegations.get(actor.as_str()) {
            Some(token) => Ok(acp::OwedAuthority::Delegation { token }),
            None => Err(acp::Error::OwedAuthorityUnavailable {
                actor: actor.to_string(),
                reason: "no key and no delegation".into(),
            }),
        }
    }

    fn command_authority(
        &self,
        actor: &Did,
        command: acp::ProviderCommand<'_>,
    ) -> acp::Result<acp::OwedAuthority> {
        if let acp::ProviderCommand::Relationship { tuple, op } = command {
            if let acp::OwedSubject::Actor { did } = &tuple.subject {
                let key = (
                    actor.to_string(),
                    tuple.object.clone(),
                    did.to_string(),
                    op == acp::OwedOp::Delete,
                );
                if let Some(token) = self.bound.get(&key) {
                    return Ok(acp::OwedAuthority::Delegation { token });
                }
            }
        }
        self.owed_authority(actor)
    }

    fn owed_journal(&self) -> Option<Arc<dyn acp::OwedJournal>> {
        self.journal.get().cloned()
    }

    async fn submit_owed_change(&self, change: &acp::OwedChange) -> acp::Submission {
        if self.unsendable.load(Ordering::SeqCst)
            || self.unsigned.contains_key(change.actor.as_str())
        {
            return acp::Submission::NotSent {
                reason: "the node could not sign the request".into(),
            };
        }
        if self.credentials.load(Ordering::SeqCst) {
            let usable = match &change.authority {
                acp::OwedAuthority::NodeKey => self.keys.contains_key(change.actor.as_str()),
                acp::OwedAuthority::Delegation { token } => !self.expired.contains_key(token),
            };
            if !usable {
                return acp::Submission::AwaitingAuthority {
                    reason: "the recorded authority no longer holds".into(),
                };
            }
        }
        acp::outbox::submit_relationship(self, change).await
    }

    async fn unregister_doc_object(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<()> {
        self.reachable()?;
        self.owners.remove(doc_id);
        Ok(())
    }
}

pub(super) fn alice() -> Did {
    Did::new("did:key:z6MkalicE").unwrap()
}

pub(super) fn bob() -> Did {
    Did::new("did:key:z6MkboB").unwrap()
}

pub(super) fn mallory() -> Did {
    Did::new("did:key:z6MkmallorY").unwrap()
}

/// The provider as every reader and writer sees it.
pub(super) fn gate(
    remote: &Arc<RemoteAcp>,
    store: Arc<RegolithStore>,
) -> Arc<acp::ExternalDocumentAcp<RegolithStore>> {
    Arc::new(acp::ExternalDocumentAcp::new(remote.clone(), store))
}

pub(super) type Runner =
    QueryRunner<LensedAutoCommitFetcher<RegolithStore>, DbTransactionRegistry<RegolithStore>>;

pub(super) struct Node {
    pub(super) db: Arc<DB<RegolithStore>>,
    pub(super) runner: Runner,
    pub(super) registry: Arc<DbTransactionRegistry<RegolithStore>>,
}

pub(super) async fn node(
    store: Arc<RegolithStore>,
    acp: Arc<dyn DocumentACP>,
    create_schema: bool,
) -> Node {
    let db = Arc::new(DB::open_from_arc(store).await.unwrap());
    if create_schema {
        for mut schema in query::parse_sdl("type Users { name: String }").unwrap() {
            schema.policy = Some(PolicyDescription::new(POLICY, RESOURCE));
            db.create_collection(schema).await.unwrap();
        }
    }
    db.set_document_acp(acp.clone());
    let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
    let runner = QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(&db)),
        DbCollectionProvider::new_arc(Arc::clone(&db)),
        Arc::clone(&registry),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db))))
    .with_acp(acp);
    Node {
        db,
        runner,
        registry,
    }
}

pub(super) fn request(query: &str, identity: Option<Did>) -> QueryRequest {
    let mut request = QueryRequest::new(query);
    request.identity = identity;
    request
}

/// The names `identity` can read.
pub(super) async fn readable(runner: &Runner, identity: Option<Did>) -> Vec<String> {
    let response = runner
        .execute(request("{ Users { name } }", identity))
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let mut names: Vec<String> = response.data.unwrap()["Users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|user| user["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    names
}

async fn create_in_txn(runner: &Runner, identity: Did, name: &str) -> String {
    let handle = runner.begin_txn(false).await.unwrap();
    let response = runner
        .execute_in_txn(
            request(
                &format!(r#"mutation {{ add_Users(input: {{name: "{name}"}}) {{ _docID }} }}"#),
                Some(identity),
            ),
            &handle,
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let doc_id = response.data.unwrap()["add_Users"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();
    // The registration may fail after the durable commit; the document stays.
    let _ = runner.commit_txn(&handle).await;
    doc_id
}

#[test]
fn an_upsert_that_updates_a_public_document_never_registers_its_caller() {
    workers::block_on(async {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let remote = RemoteAcp::new();
    let node = node(store.clone(), gate(&remote, store), true).await;
    let created = node
        .runner
        .execute(request(
            r#"mutation { add_Users(input: {name: "open"}) { _docID } }"#,
            None,
        ))
        .await;
    assert!(!created.has_errors(), "{:?}", created.errors);
    let doc_id = created.data.unwrap()["add_Users"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();

    let handle = node.runner.begin_txn(false).await.unwrap();
    let upserted = node
        .runner
        .execute_in_txn(
            request(
                r#"mutation {
                    upsert_Users(filter: {name: {_eq: "open"}}, add: {name: "fresh"}, update: {name: "taken"}) { _docID name }
                }"#,
                Some(bob()),
            ),
            &handle,
        )
        .await;
    assert!(!upserted.has_errors(), "{:?}", upserted.errors);
    assert_eq!(upserted.data.unwrap()["upsert_Users"][0]["_docID"], doc_id);
    node.runner.commit_txn(&handle).await.unwrap();

    assert_eq!(
        remote.owner(&doc_id),
        None,
        "the upsert took the document over"
    );
    assert_eq!(readable(&node.runner, Some(mallory())).await, ["taken"]);
})
    .unwrap()
}

#[test]
fn a_failed_registration_keeps_the_document_private_until_a_redrive_settles_it() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let remote = RemoteAcp::new();
        let gate = gate(&remote, store.clone());
        let node = node(store, gate.clone(), true).await;
        remote.down.store(true, Ordering::SeqCst);

        let doc_id = create_in_txn(&node.runner, alice(), "secret").await;
        let autocommit = node
            .runner
            .execute(request(
                r#"mutation { add_Users(input: {name: "also secret"}) { _docID } }"#,
                Some(alice()),
            ))
            .await;
        drop(autocommit);

        assert_eq!(remote.owner(&doc_id), None);
        assert!(readable(&node.runner, Some(mallory())).await.is_empty());
        assert!(readable(&node.runner, None).await.is_empty());
        assert_eq!(
            readable(&node.runner, Some(alice())).await,
            ["also secret", "secret"]
        );

        remote.down.store(false, Ordering::SeqCst);
        let report = gate.redrive_owed().await.unwrap();
        assert_eq!((report.settled, report.failed), (2, 0));
        assert_eq!(remote.owner(&doc_id), Some(alice()));
        assert!(readable(&node.runner, Some(mallory())).await.is_empty());
        assert_eq!(
            readable(&node.runner, Some(alice())).await,
            ["also secret", "secret"]
        );
        assert_eq!(
            gate.redrive_owed().await.unwrap(),
            acp::RedriveReport::default()
        );
    })
    .unwrap()
}

#[test]
fn a_crash_before_registration_is_settled_by_the_redrive_at_the_next_open() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let remote = RemoteAcp::new();
        let doc_id = {
            let store = Arc::new(RegolithStore::open(dir.path()).unwrap());
            let node = node(store.clone(), gate(&remote, store), true).await;
            remote.down.store(true, Ordering::SeqCst);
            let doc_id = create_in_txn(&node.runner, alice(), "before crash").await;
            node.db.close().await.unwrap();
            doc_id
        };
        remote.down.store(false, Ordering::SeqCst);
        assert_eq!(remote.owner(&doc_id), None);

        let store = Arc::new(RegolithStore::open(dir.path()).unwrap());
        let gate = gate(&remote, store.clone());
        let node = node(store, gate.clone(), false).await;
        assert!(readable(&node.runner, Some(mallory())).await.is_empty());
        let report = gate.redrive_owed().await.unwrap();
        assert_eq!((report.settled, report.failed), (1, 0));
        assert_eq!(remote.owner(&doc_id), Some(alice()));
        assert_eq!(remote.registrations.load(Ordering::SeqCst), 1);
        assert!(readable(&node.runner, Some(mallory())).await.is_empty());
        assert_eq!(
            readable(&node.runner, Some(alice())).await,
            ["before crash"]
        );
    })
    .unwrap()
}
