//! Relationship changes inside a transaction on a remote document ACP
//! provider: staged in the transaction instead of refused, effective for a
//! grantee once the provider holds them and for a revoked subject from the
//! statement that staged the revocation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crypto::Key;
use defra_core::RequestContext;
use identity::{Did, Identity as _, RawIdentity};
use regolith::sync::{HopscotchMap, SegQueue};
use vera::{ProviderError, ProviderPolicyInfo, SubjectRef, VeraProvider};

use super::{DocumentAcpProvider, Node, RelationshipChange};

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const CALLER: &str = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
const POLICY: &str = "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n";

type Map<K, V> = HopscotchMap<K, V, rapidhash::fast::RandomState>;

fn map<K, V>() -> Map<K, V>
where
    K: std::hash::Hash + Eq + Clone + 'static,
    V: Clone + 'static,
{
    HopscotchMap::with_hasher(rapidhash::fast::RandomState::default())
}

/// A remote provider kept in memory. It holds [`OWNER`]'s key, takes a
/// token `delegation:<did>` as that DID's delegation, lets only an object's
/// owner change its relationships, and rejects any other change with a
/// receipt.
struct Remote {
    policies: Map<String, String>,
    owners: Map<String, String>,
    tuples: Map<(String, String, String), ()>,
    writes: SegQueue<(bool, String, String)>,
    down: AtomicBool,
}

impl Remote {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            policies: map(),
            owners: map(),
            tuples: map(),
            writes: SegQueue::new(),
            down: AtomicBool::new(false),
        })
    }

    fn reachable(&self) -> Result<(), ProviderError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ProviderError::Unavailable("stub provider is down".into()));
        }
        Ok(())
    }

    fn holds(&self, object: &str, subject: &str) -> bool {
        self.tuples
            .contains_key(&(object.into(), "reader".into(), subject.into()))
    }

    fn take_writes(&self) -> Vec<(bool, String, String)> {
        std::iter::from_fn(|| self.writes.pop()).collect()
    }

    /// The DID a token acts as.
    fn actor(token: &str) -> &str {
        token.split_once(':').map_or(token, |(_, did)| did)
    }

    fn write(
        &self,
        add: bool,
        token: &str,
        object: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        self.reachable()?;
        if self.owners.get(object).as_deref() != Some(Self::actor(token)) {
            return Err(ProviderError::Transaction("submission was rejected".into()));
        }
        let subject = match subject {
            SubjectRef::Actor(did) => did.clone(),
            _ => "*".to_string(),
        };
        let key = (object.to_string(), relation.to_string(), subject.clone());
        let changed = if add {
            self.tuples.insert(key, ()).is_none()
        } else {
            self.tuples.remove(&key).is_some()
        };
        self.writes.push((add, object.to_string(), subject));
        Ok(changed)
    }
}

#[async_trait::async_trait]
impl VeraProvider for Remote {
    fn authorized_account(&self) -> String {
        "stub".into()
    }

    async fn create_bearer_token(&self, did: &str) -> Result<String, ProviderError> {
        self.create_bearer_token_as(&RequestContext::ambient(), did)
            .await
    }

    async fn create_bearer_token_as(
        &self,
        context: &RequestContext,
        did: &str,
    ) -> Result<String, ProviderError> {
        if self.holds_key_for(did) {
            return Ok(format!("key:{did}"));
        }
        context
            .bearer_token_for(did)
            .ok_or_else(|| ProviderError::Config(format!("no signing config found for DID: {did}")))
    }

    fn self_did(&self) -> Option<String> {
        None
    }

    fn holds_key_for(&self, did: &str) -> bool {
        did == OWNER
    }

    fn write_refused(&self, error: &ProviderError) -> bool {
        matches!(error, ProviderError::Transaction(_))
    }

    fn delegation_audience(&self) -> Option<acp::DelegationAudience> {
        Some(acp::DelegationAudience {
            worker: "did:key:stub-worker".into(),
            deployment: 1,
        })
    }

    fn caller_of_delegation(&self, token: &str) -> Option<Result<String, ProviderError>> {
        let did = token.strip_prefix("delegation:")?;
        Some(Ok(did.to_string()))
    }

    async fn create_policy(&self, policy_yaml: &str) -> Result<String, ProviderError> {
        let parsed = acp::policy_yaml::parse_policy_yaml(policy_yaml)
            .map_err(|error| ProviderError::Config(error.to_string()))?;
        let id = acp::policy_yaml::build_policy(&parsed, 0)
            .map_err(|error| ProviderError::Config(error.to_string()))?
            .id;
        self.policies.insert(id.clone(), policy_yaml.to_string());
        Ok(id)
    }

    async fn query_policy(
        &self,
        policy_id: &str,
    ) -> Result<Option<ProviderPolicyInfo>, ProviderError> {
        Ok(self.policies.get(policy_id).map(|raw| ProviderPolicyInfo {
            id: policy_id.to_string(),
            name: "Rows".into(),
            raw_policy: Some(raw),
        }))
    }

    async fn register_object(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
    ) -> Result<(), ProviderError> {
        self.reachable()?;
        self.owners
            .insert(object_id.to_string(), Self::actor(bearer_token).to_string());
        Ok(())
    }

    async fn archive_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn set_relationship(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        self.write(true, bearer_token, object_id, relation, subject)
    }

    async fn delete_relationship(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        self.write(false, bearer_token, object_id, relation, subject)
    }

    async fn query_object_owner(
        &self,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
    ) -> Result<(bool, String), ProviderError> {
        self.reachable()?;
        Ok(match self.owners.get(object_id) {
            Some(owner) => (true, owner),
            None => (false, String::new()),
        })
    }

    async fn verify_access(
        &self,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
        _permission: &str,
        actor_did: &str,
    ) -> Result<bool, ProviderError> {
        self.reachable()?;
        Ok(self.owners.get(object_id).as_deref() == Some(actor_did)
            || self.holds(object_id, actor_did)
            || self.holds(object_id, "*"))
    }
}

fn actor(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

fn carrying(did: &str, token: &str) -> RequestContext {
    actor(did).with_bearer(Some(did.to_owned()), Some(token.to_owned()))
}

fn node_identity() -> String {
    let key = crypto::generate_ed25519().unwrap();
    let identity = RawIdentity::from_ed25519(key.clone()).unwrap();
    let node_did = identity.did().unwrap().to_string();
    let public = identity.public_key_bytes();
    defra_core::signing::store_identity(
        &node_did,
        defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: key.raw().to_vec(),
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        },
    )
    .expect("the store takes the identity");
    node_did
}

/// A node on local ACP or on `remote`, its store in memory or at `data`.
async fn start(
    node_did: &str,
    remote: Option<&Arc<Remote>>,
    data: Option<&std::path::Path>,
) -> Node {
    let mut builder = Node::builder().with_node_identity_did(node_did);
    if let Some(remote) = remote {
        builder = builder.with_document_acp_provider(DocumentAcpProvider::new(remote.clone()));
    }
    if let Some(data) = data {
        builder = builder.data_path(data);
    }
    builder.build().await.unwrap()
}

async fn provision(node: &Node) {
    let policy = node.add_dac_policy_as(&actor(OWNER), POLICY).await.unwrap();
    node.add_collection(&format!(
        "type Record @policy(id: \"{policy}\", resource: \"rows\") {{ name: String }}"
    ))
    .await
    .unwrap();
}

/// A node with a policy-bound collection, on local ACP or on `remote`.
async fn node_on(remote: Option<&Arc<Remote>>) -> Node {
    let node = start(&node_identity(), remote, None).await;
    provision(&node).await;
    node
}

fn as_identity(query: impl Into<String>, did: &str) -> query::QueryRequest {
    query::QueryRequest::new(query).with_identity(Some(Did::new(did).unwrap()))
}

fn created(response: query::QueryResponse) -> String {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn create(name: &str) -> query::QueryRequest {
    as_identity(
        format!("mutation {{ add_Record(input: {{name: \"{name}\"}}) {{ _docID }} }}"),
        OWNER,
    )
}

fn names(response: query::QueryResponse) -> Vec<String> {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["Record"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["name"].as_str().unwrap().to_owned())
        .collect()
}

async fn reads(node: &Node, did: &str) -> Vec<String> {
    names(
        node.execute_request(as_identity("{ Record { name } }", did))
            .await,
    )
}

#[test]
fn a_local_provider_applies_in_the_transaction_and_a_remote_one_stages() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let owner = actor(OWNER);

        let local = node_on(None).await;
        let doc = created(local.execute_request(create("local")).await);
        let transaction = local.begin_transaction_guard(false).await.unwrap();
        let handle = transaction.handle().clone();
        let change = |delete: bool| {
            let (local, handle, owner, doc) = (&local, &handle, &owner, &doc);
            async move {
                if delete {
                    local
                        .delete_dac_actor_relationship_change_for_in_txn(
                            handle, owner, "Record", doc, "reader", READER,
                        )
                        .await
                } else {
                    local
                        .add_dac_actor_relationship_change_for_in_txn(
                            handle, owner, "Record", doc, "reader", READER,
                        )
                        .await
                }
                .unwrap()
            }
        };
        assert_eq!(
            change(false).await,
            RelationshipChange::Applied { existed: false }
        );
        assert_eq!(
            change(false).await,
            RelationshipChange::Applied { existed: true }
        );
        assert!(local
            .add_dac_actor_relationship_for_in_txn(
                &handle, &owner, "Record", &doc, "reader", READER
            )
            .await
            .unwrap());
        assert_eq!(
            change(true).await,
            RelationshipChange::Applied { existed: true }
        );
        assert_eq!(
            change(true).await,
            RelationshipChange::Applied { existed: false }
        );
        assert!(!local
            .delete_dac_actor_relationship_for_in_txn(
                &handle, &owner, "Record", &doc, "reader", READER
            )
            .await
            .unwrap());
        transaction.discard().await.unwrap();
        local.shutdown().await.unwrap();

        let remote = Remote::new();
        let node = node_on(Some(&remote)).await;
        let doc = created(node.execute_request(create("remote")).await);
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let handle = transaction.handle().clone();
        let first = node
            .add_dac_actor_relationship_change_for_in_txn(
                &handle, &owner, "Record", &doc, "reader", READER,
            )
            .await
            .unwrap();
        let RelationshipChange::Staged {
            seq,
            authority_checked: true,
        } = first
        else {
            panic!("not staged and checked: {first:?}");
        };
        // The booleans report a staged add as new and a staged delete as found.
        assert!(!first.existed_already());
        assert!(!node
            .add_dac_actor_relationship_for_in_txn(
                &handle, &owner, "Record", &doc, "reader", READER
            )
            .await
            .unwrap());
        let deleted = node
            .delete_dac_actor_relationship_change_for_in_txn(
                &handle, &owner, "Record", &doc, "reader", READER,
            )
            .await
            .unwrap();
        assert!(
            matches!(deleted, RelationshipChange::Staged { seq: later, .. } if later > seq),
            "{deleted:?}"
        );
        assert!(deleted.record_found());
        assert!(node
            .delete_dac_actor_relationship_for_in_txn(
                &handle, &owner, "Record", &doc, "reader", READER
            )
            .await
            .unwrap());
        // A caller stages in a transaction of its own through its context.
        let own = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let in_own = owner
            .clone()
            .with_txn(own.as_str().parse().ok().map(defra_core::TxnId));
        assert!(matches!(
            node.change_dac_actor_relationship_as(&in_own, "Record", &doc, "reader", READER, false)
                .await,
            Ok(RelationshipChange::Staged { .. })
        ));
        node.runner().discard_txn_as(&owner, &own).await.unwrap();
        assert!(
            remote.take_writes().is_empty(),
            "nothing is sent before the commit"
        );
        transaction.discard().await.unwrap();
        assert!(remote.take_writes().is_empty(), "a rollback owes nothing");
        // Outside a transaction the provider is asked at once.
        assert_eq!(
            node.change_dac_actor_relationship_as(&owner, "Record", &doc, "reader", READER, false)
                .await
                .unwrap(),
            RelationshipChange::Applied { existed: false }
        );
        assert!(remote.holds(&doc, READER));
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_staged_grant_waits_for_the_provider_and_a_staged_revocation_does_not() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node_on(Some(&remote)).await;
        let owner = actor(OWNER);

        // A grant on a document created in the same transaction.
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let handle = transaction.handle().clone();
        let doc = created(node.execute_request_in_txn(create("fresh"), &handle).await);
        let granted = node
            .add_dac_actor_relationship_change_for_in_txn(
                &handle, &owner, "Record", &doc, "reader", READER,
            )
            .await
            .unwrap();
        assert!(matches!(
            granted,
            RelationshipChange::Staged {
                authority_checked: true,
                ..
            }
        ));
        let read_in_txn = |did: &'static str| {
            let (node, handle) = (&node, &handle);
            async move {
                names(
                    node.execute_request_in_txn(as_identity("{ Record { name } }", did), handle)
                        .await,
                )
            }
        };
        assert!(
            read_in_txn(READER).await.is_empty(),
            "a staged grant grants nothing"
        );
        assert_eq!(read_in_txn(OWNER).await, ["fresh"]);
        assert!(reads(&node, READER).await.is_empty());
        transaction.commit().await.unwrap();

        assert_eq!(remote.owners.get(&doc).as_deref(), Some(OWNER));
        assert_eq!(
            remote.take_writes(),
            [(true, doc.clone(), READER.to_string())]
        );
        assert_eq!(reads(&node, READER).await, ["fresh"]);

        // A revocation denies its subject from the statement that staged it.
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let handle = transaction.handle().clone();
        let read_in_txn = |did: &'static str| {
            let (node, handle) = (&node, &handle);
            async move {
                names(
                    node.execute_request_in_txn(as_identity("{ Record { name } }", did), handle)
                        .await,
                )
            }
        };
        assert_eq!(read_in_txn(READER).await, ["fresh"]);
        node.delete_dac_actor_relationship_change_for_in_txn(
            &handle, &owner, "Record", &doc, "reader", READER,
        )
        .await
        .unwrap();
        assert!(read_in_txn(READER).await.is_empty());
        assert_eq!(read_in_txn(OWNER).await, ["fresh"]);
        assert_eq!(reads(&node, READER).await, ["fresh"], "not committed yet");

        // The provider is down at the commit: the revocation holds on this
        // node all the same, and the provider still holds the grant.
        remote.down.store(true, Ordering::SeqCst);
        transaction.commit().await.unwrap();
        remote.down.store(false, Ordering::SeqCst);
        assert!(remote.holds(&doc, READER));
        assert!(reads(&node, READER).await.is_empty());
        assert!(remote.take_writes().is_empty());

        // The next request that settles submits it with its own change.
        let other = created(node.execute_request(create("other")).await);
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.add_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &owner,
            "Record",
            &other,
            "reader",
            READER,
        )
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(
            remote.take_writes(),
            [
                (false, doc.clone(), READER.to_string()),
                (true, other.clone(), READER.to_string()),
            ]
        );
        assert!(!remote.holds(&doc, READER));
        assert_eq!(reads(&node, READER).await, ["other"]);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_change_is_staged_only_with_authority_to_submit_it_later() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node_on(Some(&remote)).await;
        // The caller's key never reaches the node; it owns one object.
        remote.owners.insert("bae-callers".into(), CALLER.into());

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let handle = transaction.handle().clone();
        let stage = |context: RequestContext| {
            let (node, handle) = (&node, &handle);
            async move {
                node.add_dac_actor_relationship_change_for_in_txn(
                    handle,
                    &context,
                    "Record",
                    "bae-callers",
                    "reader",
                    READER,
                )
                .await
            }
        };
        for context in [actor(CALLER), carrying(CALLER, "an.identity.token")] {
            assert!(matches!(
                stage(context).await,
                Err(crate::Error::Acp(acp::Error::OwedAuthorityUnavailable { actor, .. }))
                    if actor == CALLER
            ));
        }
        assert!(matches!(
            stage(carrying(CALLER, &format!("delegation:{OWNER}"))).await,
            Err(crate::Error::Acp(
                acp::Error::OwedAuthorityUnavailable { .. }
            ))
        ));
        let delegation = format!("delegation:{CALLER}");
        assert!(matches!(
            stage(carrying(CALLER, &delegation)).await,
            Ok(RelationshipChange::Staged { .. })
        ));
        // A change by someone the provider would refuse fails as with local ACP.
        assert!(matches!(
            node.add_dac_actor_relationship_change_for_in_txn(
                &handle,
                &actor(OWNER),
                "Record",
                "bae-callers",
                "reader",
                READER,
            )
            .await,
            Err(crate::Error::Acp(acp::Error::NotOwner { .. }))
        ));
        transaction.commit().await.unwrap();

        // Submitted after the request is gone, with the delegation it carried.
        assert_eq!(
            remote.take_writes(),
            [(true, "bae-callers".to_string(), READER.to_string())]
        );
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_restart_submits_what_the_provider_could_not_take_at_the_commit() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let directory = tempfile::tempdir().unwrap();
        let remote = Remote::new();
        let node_did = node_identity();

        let first = start(&node_did, Some(&remote), Some(directory.path())).await;
        provision(&first).await;
        let doc = created(first.execute_request(create("kept")).await);
        remote.down.store(true, Ordering::SeqCst);
        let transaction = first.begin_transaction_guard(false).await.unwrap();
        let staged = first
            .add_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &actor(OWNER),
                "Record",
                &doc,
                "reader",
                READER,
            )
            .await
            .unwrap();
        assert!(matches!(
            staged,
            RelationshipChange::Staged {
                authority_checked: false,
                ..
            }
        ));
        transaction.commit().await.unwrap();
        assert!(!remote.holds(&doc, READER));
        first.shutdown().await.unwrap();
        drop(first);

        remote.down.store(false, Ordering::SeqCst);
        let second = start(&node_did, Some(&remote), Some(directory.path())).await;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !remote.holds(&doc, READER) {
            assert!(
                std::time::Instant::now() < deadline,
                "the change was not submitted at start"
            );
            edge::testing::edge()
                .sleep(std::time::Duration::from_millis(20))
                .await;
        }
        assert_eq!(
            remote.take_writes(),
            [(true, doc.clone(), READER.to_string())]
        );
        assert_eq!(reads(&second, READER).await, ["kept"]);
        second.shutdown().await.unwrap();
    })
    .unwrap();
}
