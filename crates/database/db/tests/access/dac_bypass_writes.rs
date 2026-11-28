//! A write's DAC bypass is node access control's answer for the request's own
//! caller, decided in each attempt's transaction: an anonymous write gets
//! none from the node's default identity, and a grant or revoke that lands
//! between preparation and the attempt has the write choose its documents
//! again under the attempt's decision.

use std::sync::Arc;

use acp::DocumentACP;
use db::{
    AutoCommitMutator, DbCollectionProvider, LensedAutoCommitFetcher, NacConfig, NacManager,
    NacManagerApi, DB,
};
use identity::Did;
use query::prepared::{PreparedDacBypass, PreparedInputKind};
use query::{QueryExecutor, QueryRequest, QueryRunner};
use serde_json::{json, Value};
use storage::RegolithStore;

const SDL: &str = r#"type Users @policy(id: "bypass-policy", resource: "users") { name: String }"#;
const UPDATE: &str = r#"mutation { update_Users(filter: {}, input: {name: "changed"}) { name } }"#;

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

/// The NAC owner, who holds `bypass-dac`.
fn owner() -> Did {
    did("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
}

/// The identity whose document DAC hides from everyone else.
fn writer() -> Did {
    did("did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi")
}

/// An identity made a NAC admin, or no longer one, while a write runs.
fn admin() -> Did {
    did("did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR")
}

struct Node {
    db: Arc<DB<RegolithStore>>,
    nac: Arc<dyn NacManagerApi>,
    acp: Arc<dyn DocumentACP>,
}

impl Node {
    /// A node whose NAC the owner enabled, kept in the store so an attempt
    /// reads it in its own transaction, with one document only its writer
    /// may read under DAC.
    async fn new() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        let nac: Arc<dyn NacManagerApi> = Arc::new(NacManager::new(
            Arc::new(acp::PersistentZanzibarStore::from_store(store.clone())),
            NacConfig::new().with_enabled(),
        ));
        db.add_collection(&defra_core::RequestContext::ambient(), SDL)
            .await
            .unwrap();
        nac.initialize(Some(&owner())).await.unwrap();
        // The writer and the admin may read and write documents; only an
        // admin also bypasses DAC.
        for identity in [writer(), admin()] {
            for permission in [
                acp::nac::NodePermission::CollectionGet,
                acp::nac::NodePermission::DocumentRead,
                acp::nac::NodePermission::DocumentUpdate,
                acp::nac::NodePermission::DocumentDelete,
            ] {
                nac.add_permission_grant(&owner(), &identity, permission)
                    .await
                    .unwrap();
            }
        }
        db.set_nac_manager(nac.clone());
        let acp: Arc<dyn DocumentACP> = Arc::new(acp::ZanzibarDocumentACP::new(Arc::new(
            acp::PersistentZanzibarStore::from_store(store),
        )));
        db.set_document_acp(acp.clone());
        let node = Self { db, nac, acp };
        let added = node
            .run(
                node.runner(),
                r#"mutation { add_Users(input: {name: "private"}) { _docID } }"#,
                Some(writer()),
            )
            .await;
        assert!(added["add_Users"][0]["_docID"].is_string());
        node
    }

    fn runner(&self) -> QueryRunner<LensedAutoCommitFetcher<RegolithStore>> {
        QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(self.db.clone()),
            DbCollectionProvider::new_arc(self.db.clone()),
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(self.db.clone())))
        .with_acp(self.acp.clone())
    }

    async fn run(&self, runner: impl QueryExecutor, query: &str, caller: Option<Did>) -> Value {
        let response = runner.execute(request(query, caller)).await;
        assert!(response.errors.is_empty(), "{query}: {:?}", response.errors);
        response.data.unwrap()
    }
}

fn request(query: &str, caller: Option<Did>) -> QueryRequest {
    let mut request = QueryRequest::new(query);
    request.identity = caller;
    request
}

#[test]
fn an_anonymous_write_gets_no_bypass_from_the_default_identity() {
    workers::block_on(async {
        let node = Node::new().await;
        let runner = || node.runner().with_default_identity(owner());

        assert_eq!(
            node.run(runner(), UPDATE, None).await,
            json!({ "update_Users": [] }),
            "an anonymous write acts as the default identity under DAC, without its bypass"
        );
        assert_eq!(
            node.run(runner(), "{ Users { name } }", Some(writer()))
                .await,
            json!({ "Users": [{ "name": "private" }] }),
            "the anonymous write changed nothing"
        );
        assert_eq!(
            node.run(runner(), UPDATE, Some(owner())).await,
            json!({ "update_Users": [{ "name": "changed" }] }),
            "the owner making the request gets its bypass"
        );
    })
    .unwrap();
}

/// Run `query` as `caller`, prepared before `between` runs and attempted
/// after it, as the node's boundary does: each attempt that asks for an
/// input gets it before the next. Returns what the write answered and the
/// inputs its attempts asked for.
async fn prepared_then<B, Fut>(
    node: &Node,
    query: &str,
    caller: Did,
    between: B,
) -> (Value, Vec<PreparedInputKind>)
where
    B: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let runner = node.runner();
    let request = request(query, Some(caller));
    let mut prepared = runner.prepare_request(&request).await.unwrap();
    between().await;
    let mut asked = Vec::new();
    loop {
        let response = runner
            .execute_prepared(request.clone(), prepared.clone())
            .await;
        let Some(input) = response.retry_input().cloned() else {
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            return (response.data.unwrap(), asked);
        };
        asked.push(input.kind());
        prepared = runner
            .resolve_retry_input(&request, prepared, &input)
            .await
            .unwrap();
    }
}

#[test]
fn a_bypass_granted_after_preparation_reaches_every_document() {
    workers::block_on(async {
        let node = Node::new().await;
        let (answer, asked) = prepared_then(&node, UPDATE, admin(), || async {
            assert!(node.nac.add_admin(&owner(), &admin()).await.unwrap());
        })
        .await;
        assert_eq!(answer, json!({ "update_Users": [{ "name": "changed" }] }));
        assert_eq!(
            asked,
            [PreparedInputKind::DacBypass],
            "the attempt that decided the bypass chose the documents again"
        );
    })
    .unwrap();
}

#[test]
fn a_bypass_revoked_after_preparation_reaches_only_what_dac_grants() {
    workers::block_on(async {
        let node = Node::new().await;
        assert!(node.nac.add_admin(&owner(), &admin()).await.unwrap());
        let (answer, asked) = prepared_then(&node, UPDATE, admin(), || async {
            assert!(node.nac.remove_admin(&owner(), &admin()).await.unwrap());
        })
        .await;
        assert_eq!(answer, json!({ "update_Users": [] }));
        assert_eq!(asked, [PreparedInputKind::DacBypass]);
        assert_eq!(
            node.run(node.runner(), "{ Users { name } }", Some(writer()))
                .await,
            json!({ "Users": [{ "name": "private" }] })
        );
    })
    .unwrap();
}

#[test]
fn an_attempt_that_decides_the_prepared_bypass_asks_for_nothing() {
    let prepared = PreparedDacBypass {
        caller: Some(admin()),
        bypassed: true,
    };
    assert!(prepared.stale(true).is_none());
    let again = prepared.stale(false).unwrap();
    assert_eq!(PreparedDacBypass::decided(&again), Some(false));
}
