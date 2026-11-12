//! Who is shown a row of the update journal: a deleted or purged document of
//! a policy-backed collection is shown to nobody, and access is decided in
//! the transaction that read the page.

mod decisions;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use acp::DocumentACP;
use db::{
    AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB,
};
use identity::Did;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use schema::PolicyDescription;
use serde_json::Value;
use storage::RegolithStore;

const POLICY: &str = "update-policy";
const RESOURCE: &str = "users";
const UPDATES: &str =
    r#"{ _documentUpdates(collection: "Users") { head next entries { cursor docID cid } } }"#;

fn owner() -> Did {
    Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap()
}

fn stranger() -> Did {
    Did::new("did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi").unwrap()
}

/// How a request reaches the journal: on a read transaction of its own, as a
/// node serves it, or through the auto-committing fetcher alone.
#[derive(Clone, Copy, Debug)]
enum Path {
    Registry,
    AutoCommit,
}

struct Node {
    db: Arc<DB<RegolithStore>>,
    acp: Arc<dyn DocumentACP>,
}

impl Node {
    /// A store whose document ACP is the node's own local one, kept in the
    /// store, so a delete unregisters its document in the delete transaction.
    async fn new(policy: bool) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        for mut schema in query::parse_sdl("type Users { name: String }").unwrap() {
            if policy {
                schema.policy = Some(PolicyDescription::new(POLICY, RESOURCE));
            }
            db.create_collection(schema).await.unwrap();
        }
        let acp: Arc<dyn DocumentACP> = Arc::new(acp::ZanzibarDocumentACP::new(Arc::new(
            acp::PersistentZanzibarStore::from_store(store),
        )));
        db.set_document_acp(acp.clone());
        Self { db, acp }
    }

    fn runner(&self, path: Path, acp: Arc<dyn DocumentACP>) -> Box<dyn QueryExecutor> {
        let fetcher = LensedAutoCommitFetcher::new(self.db.clone());
        let provider = DbCollectionProvider::new_arc(self.db.clone());
        let mutator = Arc::new(AutoCommitMutator::new(self.db.clone()));
        match path {
            Path::Registry => Box::new(
                QueryRunner::with_arc_registry_and_provider(
                    fetcher,
                    provider,
                    Arc::new(DbTransactionRegistry::new(self.db.clone())),
                )
                .with_mutator(mutator)
                .with_acp(acp),
            ),
            Path::AutoCommit => Box::new(
                QueryRunner::with_provider(fetcher, provider)
                    .with_mutator(mutator)
                    .with_acp(acp),
            ),
        }
    }

    async fn run(&self, path: Path, query: &str, identity: Option<Did>) -> Value {
        run(
            self.runner(path, self.acp.clone()).as_ref(),
            query,
            identity,
        )
        .await
    }

    /// A document its owner created as `name` and then updated once: one
    /// journal row.
    async fn updated_document(&self, name: &str) -> String {
        let created = self
            .run(
                Path::Registry,
                &format!(r#"mutation {{ add_Users(input: {{name: "{name}"}}) {{ _docID }} }}"#),
                Some(owner()),
            )
            .await;
        let id = created["add_Users"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string();
        self.update(&id, &format!("{name}, updated")).await;
        id
    }

    async fn update(&self, id: &str, name: &str) {
        self.run(
            Path::Registry,
            &format!(
                r#"mutation {{ update_Users(docID: "{id}", input: {{name: "{name}"}}) {{ _docID }} }}"#
            ),
            Some(owner()),
        )
        .await;
    }

    async fn delete(&self, id: &str) {
        let deleted = self
            .run(
                Path::Registry,
                &format!(r#"mutation {{ delete_Users(docID: "{id}") {{ _docID }} }}"#),
                Some(owner()),
            )
            .await;
        assert_eq!(deleted["delete_Users"][0]["_docID"], id);
    }

    /// The journal page as `identity` reads it: its `next`, and the document
    /// of each entry.
    async fn page(&self, path: Path, identity: Option<Did>) -> (Value, Vec<String>) {
        let data = self.run(path, UPDATES, identity).await;
        shown(&data)
    }
}

fn shown(data: &Value) -> (Value, Vec<String>) {
    let page = &data["_documentUpdates"];
    (
        page["next"].clone(),
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["docID"].as_str().unwrap().to_string())
            .collect(),
    )
}

async fn run(runner: &dyn QueryExecutor, query: &str, identity: Option<Did>) -> Value {
    let mut request = QueryRequest::new(query);
    request.identity = identity;
    let response = runner.execute(request).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.unwrap()
}

#[test]
fn a_deleted_private_documents_updates_are_shown_to_nobody() {
    workers::block_on(async {
        for path in [Path::Registry, Path::AutoCommit] {
            let node = Node::new(true).await;
            let id = node.updated_document("removed").await;
            let kept = node.updated_document("kept").await;

            let (next, entries) = node.page(path, Some(owner())).await;
            assert_eq!(next, "2", "{path:?}");
            assert_eq!(entries, [id.clone(), kept.clone()], "{path:?}");
            for reader in [None, Some(stranger())] {
                let (next, entries) = node.page(path, reader).await;
                assert_eq!(next, "2", "{path:?}");
                assert!(entries.is_empty(), "{path:?}: private while it lives");
            }

            node.delete(&id).await;
            assert!(
                !node
                    .acp
                    .is_doc_registered(POLICY, RESOURCE, &id)
                    .await
                    .unwrap(),
                "the delete unregisters the document, which then reads as public"
            );
            for reader in [None, Some(stranger())] {
                let (next, entries) = node.page(path, reader.clone()).await;
                assert_eq!(next, "2", "{path:?}: a hidden row still advances the page");
                assert!(entries.is_empty(), "{path:?} as {reader:?}");
            }
            let (_, entries) = node.page(path, Some(owner())).await;
            assert_eq!(entries, [kept], "{path:?}: only the deleted document goes");
        }
    })
    .unwrap();
}

#[test]
fn a_purged_private_documents_updates_are_shown_to_nobody() {
    workers::block_on(async {
        for (path, delete_first) in [
            (Path::Registry, true),
            (Path::Registry, false),
            (Path::AutoCommit, true),
            (Path::AutoCommit, false),
        ] {
            let node = Node::new(true).await;
            let id = node.updated_document("removed").await;
            let kept = node.updated_document("kept").await;
            if delete_first {
                node.delete(&id).await;
            }
            let purged = node
                .db
                .purge_by_doc_ids("Users", &[id.parse::<document::DocID>().unwrap()], None)
                .await
                .unwrap();
            assert_eq!(purged, 1);
            let case = format!("{path:?}, deleted first: {delete_first}");
            for reader in [None, Some(stranger())] {
                let (next, entries) = node.page(path, reader.clone()).await;
                assert_eq!(next, "2", "{case}");
                assert!(entries.is_empty(), "{case}, as {reader:?}");
            }
            let (_, entries) = node.page(path, Some(owner())).await;
            assert_eq!(entries, [kept], "{case}");
        }
    })
    .unwrap();
}

#[test]
fn a_collection_without_a_policy_still_lists_a_deleted_documents_updates() {
    workers::block_on(async {
        for path in [Path::Registry, Path::AutoCommit] {
            let node = Node::new(false).await;
            let id = node.updated_document("removed").await;
            node.delete(&id).await;
            let (next, entries) = node.page(path, None).await;
            assert_eq!(next, "1", "{path:?}");
            assert_eq!(entries, [id], "{path:?}");
        }
    })
    .unwrap();
}

#[test]
fn one_page_decides_each_document_once() {
    workers::block_on(async {
        let node = Node::new(true).await;
        let id = node.updated_document("one").await;
        for name in ["third", "fourth"] {
            node.update(&id, name).await;
        }
        // A request's own read transaction binds the provider, and the page
        // binds it to that same transaction again.
        for (path, bindings) in [(Path::Registry, 2), (Path::AutoCommit, 1)] {
            let counting = Arc::new(decisions::Decisions::new(node.acp.clone()));
            let data = run(
                node.runner(path, counting.clone()).as_ref(),
                UPDATES,
                Some(owner()),
            )
            .await;
            assert_eq!(shown(&data).1.len(), 3, "{path:?}");
            assert_eq!(counting.bound.load(Ordering::SeqCst), bindings, "{path:?}");
            assert_eq!(counting.checks.load(Ordering::SeqCst), 1, "{path:?}");
        }
    })
    .unwrap();
}

#[test]
fn a_page_in_a_transaction_decides_at_that_transactions_snapshot() {
    workers::block_on(async {
        let node = Node::new(true).await;
        let id = node.updated_document("removed").await;
        let runner = node.runner(Path::Registry, node.acp.clone());
        let before_delete = runner.begin_txn(true).await.unwrap();
        node.delete(&id).await;

        for (reader, expected) in [
            (Some(owner()), vec![id.clone()]),
            (Some(stranger()), vec![]),
            (None, vec![]),
        ] {
            let mut request = QueryRequest::new(UPDATES);
            request.identity = reader.clone();
            let response = runner.execute_in_txn(request, &before_delete).await;
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            assert_eq!(
                shown(&response.data.unwrap()).1,
                expected,
                "as {reader:?}: the row and its access are both read before the delete"
            );
        }
        runner.discard_txn(&before_delete).await.unwrap();
        assert!(node.page(Path::Registry, Some(owner())).await.1.is_empty());
    })
    .unwrap();
}

#[test]
fn a_page_in_a_writing_transaction_does_not_fail_it_when_its_document_is_purged() {
    workers::block_on(async {
        let node = Node::new(true).await;
        let id = node.updated_document("removed").await;
        let runner = node.runner(Path::Registry, node.acp.clone());
        let writing = runner.begin_txn(false).await.unwrap();
        let mut page = QueryRequest::new(UPDATES);
        page.identity = Some(owner());
        let response = runner.execute_in_txn(page, &writing).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(shown(&response.data.unwrap()).1, std::slice::from_ref(&id));

        let purged = node
            .db
            .purge_by_doc_ids("Users", &[id.parse::<document::DocID>().unwrap()], None)
            .await
            .unwrap();
        assert_eq!(purged, 1);

        let mut write =
            QueryRequest::new(r#"mutation { add_Users(input: {name: "unrelated"}) { _docID } }"#);
        write.identity = Some(owner());
        let response = runner.execute_in_txn(write, &writing).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        runner
            .commit_txn(&writing)
            .await
            .expect("the page decided nothing the transaction wrote");
    })
    .unwrap();
}

#[test]
fn a_delete_during_the_page_hides_the_row_from_a_provider_outside_its_transaction() {
    workers::block_on(async {
        for path in [Path::Registry, Path::AutoCommit] {
            // No provider on the store: the mutator writes unchecked, and the
            // provider below stands outside every transaction.
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let db = Arc::new(DB::from_arc(store).unwrap());
            for mut schema in query::parse_sdl("type Users { name: String }").unwrap() {
                schema.policy = Some(PolicyDescription::new(POLICY, RESOURCE));
                db.create_collection(schema).await.unwrap();
            }
            let node = Node {
                db: db.clone(),
                acp: Arc::new(acp::LocalDocumentACP::new(Arc::new(
                    acp::MemoryAcpStore::new(),
                ))),
            };
            let removed = node.updated_document("removed").await;
            let kept = node.updated_document("kept").await;
            for id in [&removed, &kept] {
                assert!(node
                    .acp
                    .is_doc_registered(POLICY, RESOURCE, id)
                    .await
                    .unwrap());
            }
            let racing: Arc<dyn DocumentACP> = Arc::new(decisions::DeletesMidPage::new(
                node.acp.clone(),
                db,
                removed.clone(),
            ));

            let data = run(node.runner(path, racing).as_ref(), UPDATES, None).await;
            let (next, entries) = shown(&data);
            assert_eq!(next, "2", "{path:?}");
            assert!(
                entries.is_empty(),
                "{path:?}: deleted after the page began, public to the provider: {entries:?}"
            );
            let (_, entries) = node.page(path, Some(owner())).await;
            assert_eq!(entries, [kept], "{path:?}");
        }
    })
    .unwrap();
}
