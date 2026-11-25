//! A batched create on a parent-gated collection, against a provider that
//! cannot join the transaction, owes two things: the pending record the
//! create writes, which carries the parent, and the commit registration the
//! batch owes for the document, which names it alone. The commit
//! registration only redrives the record (`redrive_registrations`), so the
//! provider is sent exactly one registration per document, under its parent,
//! and never a parentless one.

use acp::{DocumentACP, Identity, RegistrationParent};
use async_trait::async_trait;
use db::{DbCollectionProvider, LensedAutoCommitFetcher, DB};
use query::{QueryExecutor, QueryRequest};
use regolith::sync::Atom;
use std::sync::Arc;
use storage::RegolithStore;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const POLICY: &str = "gated-policy";
const SDL: &str = r#"
    type Job @policy(id: "gated-policy", resource: "jobs", parentField: "parent_id") {
        parent_id: String! @immutable
        name: String
    }
"#;

/// One registration the provider was sent.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sent {
    /// A registration with no parent.
    Plain(String),
    /// A registration under `parent`.
    UnderParent { doc: String, parent: String },
}

/// A remote provider that admits every parent and records what it is sent.
#[derive(Default)]
struct RecordingRemote {
    sent: Atom<Vec<Sent>>,
}

impl RecordingRemote {
    fn push(&self, sent: Sent) {
        self.sent.rcu(|seen| {
            let mut next = seen.clone();
            next.push(sent.clone());
            next
        });
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.load_clone()
    }

    fn holds(&self, doc_id: &str) -> bool {
        self.sent().iter().any(|sent| match sent {
            Sent::Plain(doc) | Sent::UnderParent { doc, .. } => doc == doc_id,
        })
    }
}

#[async_trait]
impl DocumentACP for RecordingRemote {
    fn supports_transactions(&self) -> bool {
        false
    }

    async fn register_doc_object(
        &self,
        _identity: &identity::Did,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<()> {
        self.push(Sent::Plain(doc_id.to_owned()));
        Ok(())
    }

    async fn registration_parent_rule(
        &self,
        _policy_id: &str,
        _child: &str,
        id: &str,
    ) -> acp::Result<(RegistrationParent, String)> {
        Ok((
            RegistrationParent {
                resource: "nodes".into(),
                object_id: id.into(),
            },
            "owner".into(),
        ))
    }

    async fn check_named_permission(
        &self,
        _subject: &identity::Did,
        _permission: &str,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(true)
    }

    async fn register_under_parent(
        &self,
        _identity: &identity::Did,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
        parent: &RegistrationParent,
    ) -> acp::Result<()> {
        self.push(Sent::UnderParent {
            doc: object_id.to_owned(),
            parent: parent.object_id.clone(),
        });
        Ok(())
    }

    async fn is_doc_registered(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(self.holds(doc_id))
    }

    async fn check_doc_access(
        &self,
        _identity: &Identity,
        _permission: acp::DocumentPermission,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        Ok(true)
    }

    async fn add_actor_relationship(
        &self,
        _requestor: &identity::Did,
        _target: &identity::Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn delete_actor_relationship(
        &self,
        _requestor: &identity::Did,
        _target: &identity::Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn unregister_doc_object(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
        Ok(())
    }
}

fn collect_doc_ids(value: &serde_json::Value, ids: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(id) = map.get("_docID").and_then(serde_json::Value::as_str) {
                ids.push(id.to_owned());
            }
            for child in map.values() {
                collect_doc_ids(child, ids);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_doc_ids(child, ids);
            }
        }
        _ => {}
    }
}

/// Run `mutation` as the owner through the engine, resolving its remote
/// inputs outside the attempt as a node does, and return what the provider
/// was sent with the documents the mutation returned.
async fn run_as_owner(mutation: &str) -> (Vec<Sent>, Vec<String>) {
    let remote = Arc::new(RecordingRemote::default());
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    db.set_document_acp(Arc::new(acp::ExternalDocumentAcp::new(
        remote.clone(),
        db.store().clone(),
    )));
    let db = Arc::new(db);
    db.add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .unwrap();
    let runner = query::QueryRunner::with_provider(
        LensedAutoCommitFetcher::new(db.clone()),
        DbCollectionProvider::new_arc(db.clone()),
    )
    .with_mutator(Arc::new(db::write::autocommit::AutoCommitMutator::new(
        db.clone(),
    )));
    let owner = identity::Did::new(OWNER).unwrap();
    let request = QueryRequest::new(mutation).with_identity(Some(owner.clone()));
    let response = defra_core::current_identity::with_scoped_identity(Some(OWNER.into()), async {
        defra_core::signing::scope_broadcast_creator_did(Some(OWNER.into()), async {
            let mut prepared = runner.prepare_request(&request).await.unwrap();
            assert!(prepared.is_some(), "a top-level batch is request-prepared");
            loop {
                let response = runner
                    .execute_prepared(request.clone(), prepared.clone())
                    .await;
                match response.retry_input() {
                    Some(input) => {
                        prepared = runner
                            .resolve_retry_input(&request, prepared.clone(), input)
                            .await
                            .unwrap();
                    }
                    None => break response,
                }
            }
        })
        .await
    })
    .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let mut ids = Vec::new();
    collect_doc_ids(&response.data.unwrap(), &mut ids);
    let gate = db.document_acp().unwrap();
    for id in &ids {
        assert!(
            gate.is_doc_registered(POLICY, "jobs", id).await.unwrap(),
            "{id} is registered and nothing is left owed for it"
        );
    }
    (remote.sent(), ids)
}

fn under(doc: &str, parent: &str) -> Sent {
    Sent::UnderParent {
        doc: doc.into(),
        parent: parent.into(),
    }
}

#[test]
fn batched_parent_gated_creates_send_one_parent_bound_registration_each() {
    workers::block_on(async {
        let (sent, ids) = run_as_owner(
            r#"mutation {
                first: add_Job(input: {parent_id: "node-a", name: "one"}) { _docID }
                second: add_Job(input: {parent_id: "node-b", name: "two"}) { _docID }
            }"#,
        )
        .await;
        assert_eq!(ids.len(), 2);
        assert_eq!(
            sent,
            vec![under(&ids[0], "node-a"), under(&ids[1], "node-b")]
        );
    })
    .unwrap()
}

#[test]
fn a_batched_upsert_create_sends_one_parent_bound_registration() {
    workers::block_on(async {
        let (sent, ids) = run_as_owner(
            r#"mutation {
                first: add_Job(input: {parent_id: "node-a", name: "one"}) { _docID }
                second: upsert_Job(
                    filter: {name: {_eq: "two"}},
                    add: {parent_id: "node-b", name: "two"},
                    update: {name: "two"}
                ) { _docID }
            }"#,
        )
        .await;
        assert_eq!(ids.len(), 2);
        assert_eq!(
            sent,
            vec![under(&ids[0], "node-a"), under(&ids[1], "node-b")]
        );
    })
    .unwrap()
}
