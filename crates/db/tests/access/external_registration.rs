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
use regolith::sync::HopscotchMap;
use schema::PolicyDescription;
use storage::RegolithStore;

const POLICY: &str = "remote-policy";
const RESOURCE: &str = "users";

/// A remote provider: unregistered documents are public, a registered one is
/// readable by its owner only, and every call fails while it is down.
#[derive(Default)]
struct RemoteAcp {
    owners: HopscotchMap<String, Did, rapidhash::fast::RandomState>,
    down: AtomicBool,
    registrations: AtomicUsize,
}

impl RemoteAcp {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            owners: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
            ..Self::default()
        })
    }

    fn reachable(&self) -> acp::Result<()> {
        if self.down.load(Ordering::SeqCst) {
            return Err(acp::Error::Storage("provider unreachable".into()));
        }
        Ok(())
    }

    fn owner(&self, doc_id: &str) -> Option<Did> {
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
        Ok(match self.owners.get(doc_id) {
            None => true,
            Some(owner) => matches!(identity, Identity::Authenticated(did) if *did == owner),
        })
    }

    async fn add_actor_relationship(
        &self,
        _requestor: &Did,
        _target: &Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        self.reachable()?;
        Ok(false)
    }

    async fn delete_actor_relationship(
        &self,
        _requestor: &Did,
        _target: &Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        self.reachable()?;
        Ok(false)
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

fn alice() -> Did {
    Did::new("did:key:z6MkalicE").unwrap()
}

fn bob() -> Did {
    Did::new("did:key:z6MkboB").unwrap()
}

fn mallory() -> Did {
    Did::new("did:key:z6MkmallorY").unwrap()
}

/// The provider as every reader and writer sees it.
fn gate(
    remote: &Arc<RemoteAcp>,
    store: Arc<RegolithStore>,
) -> Arc<acp::ExternalDocumentAcp<RegolithStore>> {
    Arc::new(acp::ExternalDocumentAcp::new(remote.clone(), store))
}

type Runner =
    QueryRunner<LensedAutoCommitFetcher<RegolithStore>, DbTransactionRegistry<RegolithStore>>;

struct Node {
    db: Arc<DB<RegolithStore>>,
    runner: Runner,
}

async fn node(store: Arc<RegolithStore>, acp: Arc<dyn DocumentACP>, create_schema: bool) -> Node {
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
        registry,
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db))))
    .with_acp(acp);
    Node { db, runner }
}

fn request(query: &str, identity: Option<Did>) -> QueryRequest {
    let mut request = QueryRequest::new(query);
    request.identity = identity;
    request
}

/// The names `identity` can read.
async fn readable(runner: &Runner, identity: Option<Did>) -> Vec<String> {
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
