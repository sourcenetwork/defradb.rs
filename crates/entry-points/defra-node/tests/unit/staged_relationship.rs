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
use vera::{
    PolicyAction, PolicyCommand, PolicySubject, ProviderError, ProviderPolicyInfo, SubjectRef,
    VeraProvider,
};

use super::{DocumentAcpProvider, Node, RelationshipChange};

pub(super) const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
pub(super) const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
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

/// A remote provider kept in memory. It holds [`OWNER`]'s key and those in
/// `keys`, takes a token `delegation:<did>` as that DID's delegation and a
/// token `request:<did>|<command>` ([`request_token`]) as that DID's
/// delegation bound to one command, lets only an object's owner change its
/// relationships, and rejects any other change with a receipt. It proves
/// its decisions, as vera.rs does: only an owner or a `writer` updates or
/// deletes an object, and a `reader` also holds every other permission.
pub(super) struct Remote {
    policies: Map<String, String>,
    pub(super) owners: Map<String, String>,
    tuples: Map<(String, String, String), ()>,
    writes: SegQueue<(bool, String, String)>,
    /// Further DIDs whose keys the node holds.
    pub(super) keys: Map<String, ()>,
    /// Every delegation bound to one request has expired.
    pub(super) lapsed: AtomicBool,
    /// How many access decisions the provider was asked to record.
    pub(super) decisions: std::sync::atomic::AtomicU64,
    pub(super) down: AtomicBool,
    /// Objects whose relationship writes fail as unreachable while every
    /// other call answers: what keeps a change owed on them while the
    /// node's drainer runs.
    cut_off: Map<String, ()>,
    /// Its newest verified state is past the freshness limit: every decision
    /// fails as the vera.rs light client's does.
    pub(super) stale: AtomicBool,
    /// The provider gives the node nothing to check a delegation against,
    /// as the Cosmos provider does: it is passed its caller's token as it
    /// is.
    pub(super) unverifying: AtomicBool,
    /// A relationship write does not answer until `released`, and then
    /// fails as not delivered.
    stalled: AtomicBool,
    released: regolith::sync::Event,
    /// An owner read waits, once, until `owner_reads_released`, after
    /// setting `owner_read_waiting`.
    pub(super) hold_owner_read: AtomicBool,
    pub(super) owner_read_waiting: regolith::sync::Event,
    pub(super) owner_reads_released: regolith::sync::Event,
    /// Objects whose access checks fail as unavailable while every other
    /// call answers.
    pub(super) unchecked: Map<String, ()>,
}

/// The words a delegation bound to one request names `command` by.
fn named(command: &PolicyCommand<'_>) -> String {
    let subject = |subject: &PolicySubject<'_>| match subject {
        PolicySubject::Actor(SubjectRef::Actor(did)) => did.clone(),
        PolicySubject::Actor(_) => "*".to_owned(),
        PolicySubject::Structured {
            resource,
            object_id,
            relation,
            ..
        } => format!("{resource}:{object_id}#{relation}"),
    };
    let object = command.object_id;
    match &command.action {
        PolicyAction::RegisterObject => format!("register {object}"),
        PolicyAction::RegisterUnderParent {
            resource,
            object_id,
        } => format!("register {object} under {resource}:{object_id}"),
        PolicyAction::SetRelationship {
            relation,
            subject: on,
        } => format!("set {object} {relation} {}", subject(on)),
        PolicyAction::DeleteRelationship {
            relation,
            subject: on,
        } => format!("delete {object} {relation} {}", subject(on)),
    }
}

/// `did`'s delegation bound to the one command `command` names
/// (`register <object>`, or `set`/`delete <object> <relation> <subject>`).
pub(super) fn request_token(did: &str, command: &str) -> String {
    format!("request:{did}|{command}")
}

impl Remote {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            policies: map(),
            owners: map(),
            tuples: map(),
            writes: SegQueue::new(),
            keys: map(),
            lapsed: AtomicBool::new(false),
            decisions: std::sync::atomic::AtomicU64::new(0),
            down: AtomicBool::new(false),
            cut_off: map(),
            stale: AtomicBool::new(false),
            unverifying: AtomicBool::new(false),
            stalled: AtomicBool::new(false),
            released: regolith::sync::Event::new(),
            hold_owner_read: AtomicBool::new(false),
            owner_read_waiting: regolith::sync::Event::new(),
            owner_reads_released: regolith::sync::Event::new(),
            unchecked: map(),
        })
    }

    /// Fail every relationship write on `object` as unreachable, or take
    /// them again.
    pub(super) fn cut_off(&self, object: &str, cut: bool) {
        if cut {
            self.cut_off.insert(object.to_string(), ());
        } else {
            self.cut_off.remove(object);
        }
    }

    fn reachable(&self) -> Result<(), ProviderError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ProviderError::Unavailable("stub provider is down".into()));
        }
        Ok(())
    }

    /// Hold a relationship write while the provider is stalled.
    async fn stall(&self) -> Result<(), ProviderError> {
        if self.stalled.load(Ordering::SeqCst) {
            self.released.wait().await;
            return Err(ProviderError::Unavailable("stub provider stalled".into()));
        }
        Ok(())
    }

    pub(super) fn holds(&self, object: &str, subject: &str) -> bool {
        self.holds_as(object, "reader", subject)
    }

    fn holds_as(&self, object: &str, relation: &str, subject: &str) -> bool {
        self.tuples
            .contains_key(&(object.into(), relation.into(), subject.into()))
    }

    /// The owner of `object` gives `subject` the relation `relation` on it,
    /// or takes it back, and the provider finalizes that.
    pub(super) fn finalize(&self, object: &str, relation: &str, subject: &str, held: bool) {
        let key = (
            object.to_string(),
            relation.to_string(),
            subject.to_string(),
        );
        if held {
            self.tuples.insert(key, ());
        } else {
            self.tuples.remove(&key);
        }
    }

    /// How many writes reached the provider and were not taken yet.
    fn pending_writes(&self) -> usize {
        self.writes.len()
    }

    pub(super) fn take_writes(&self) -> Vec<(bool, String, String)> {
        std::iter::from_fn(|| self.writes.pop()).collect()
    }

    /// The DID a token acts as.
    fn actor(token: &str) -> &str {
        let did = token.split_once(':').map_or(token, |(_, did)| did);
        did.split_once('|').map_or(did, |(did, _)| did)
    }

    /// What the provider itself answers a token bound to one request that
    /// is sent with another command, or after it expired: a rejection.
    fn executes(&self, token: &str, command: &PolicyCommand<'_>) -> Result<(), ProviderError> {
        match self.caller_of_request_delegation(token, command) {
            Some(Err(_)) => Err(ProviderError::Transaction("submission was rejected".into())),
            _ => Ok(()),
        }
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
        if self.cut_off.contains_key(object) {
            return Err(ProviderError::Unavailable(format!(
                "stub provider is cut off from {object}"
            )));
        }
        if self.owners.get(object).as_deref() != Some(Self::actor(token)) {
            return Err(ProviderError::Transaction("submission was rejected".into()));
        }
        let subject = match subject {
            SubjectRef::Actor(did) => did.clone(),
            _ => "*".to_string(),
        };
        let key = (object.to_string(), relation.to_string(), subject.clone());
        // Told before the tuple changes: a test that sees the tuple change
        // finds the write among the notifications.
        self.writes.push((add, object.to_string(), subject));
        let changed = if add {
            self.tuples.insert(key, ()).is_none()
        } else {
            self.tuples.remove(&key).is_some()
        };
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
        did == OWNER || self.keys.contains_key(did)
    }

    fn write_refused(&self, error: &ProviderError) -> bool {
        matches!(error, ProviderError::Transaction(_))
    }

    fn takes_subject_commands(&self) -> bool {
        true
    }

    fn proves_decisions(&self) -> bool {
        true
    }

    fn delegation_audience(&self) -> Option<acp::DelegationAudience> {
        if self.unverifying.load(Ordering::SeqCst) {
            return None;
        }
        Some(acp::DelegationAudience {
            worker: "did:key:stub-worker".into(),
            deployment: 1,
            genesis_id: None,
        })
    }

    fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<Result<acp::AuthenticatedCaller, ProviderError>> {
        if self.unverifying.load(Ordering::SeqCst) {
            return None;
        }
        let did = token.strip_prefix("delegation:")?;
        Some(Ok(acp::AuthenticatedCaller {
            did: identity::Did::new(did).expect("fixture DID"),
            expires_at: u64::MAX,
        }))
    }

    fn caller_of_request_delegation(
        &self,
        token: &str,
        command: &PolicyCommand<'_>,
    ) -> Option<Result<String, ProviderError>> {
        let (did, names) = token.strip_prefix("request:")?.split_once('|')?;
        Some(if self.lapsed.load(Ordering::SeqCst) {
            Err(ProviderError::Config("the delegation has expired".into()))
        } else if names != named(command) {
            Err(ProviderError::Config(
                "the delegation is bound to another request".into(),
            ))
        } else {
            Ok(did.to_string())
        })
    }

    async fn create_policy(
        &self,
        policy_yaml: &str,
        _nonce: Option<acp::PolicyNonce>,
    ) -> Result<String, ProviderError> {
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
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(), ProviderError> {
        self.reachable()?;
        self.executes(
            bearer_token,
            &PolicyCommand {
                policy_id,
                resource,
                object_id,
                action: PolicyAction::RegisterObject,
            },
        )?;
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
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        self.stall().await?;
        self.executes(
            bearer_token,
            &PolicyCommand {
                policy_id,
                resource,
                object_id,
                action: PolicyAction::SetRelationship {
                    relation,
                    subject: PolicySubject::Actor(subject),
                },
            },
        )?;
        self.write(true, bearer_token, object_id, relation, subject)
    }

    async fn delete_relationship(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        self.executes(
            bearer_token,
            &PolicyCommand {
                policy_id,
                resource,
                object_id,
                action: PolicyAction::DeleteRelationship {
                    relation,
                    subject: PolicySubject::Actor(subject),
                },
            },
        )?;
        self.write(false, bearer_token, object_id, relation, subject)
    }

    async fn set_relationship_subject_as(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
        relation: &str,
        _kind: u8,
        subject_resource: &str,
        subject_object_id: &str,
        subject_relation: &str,
    ) -> Result<bool, ProviderError> {
        let subject = SubjectRef::Actor(format!(
            "{subject_resource}:{subject_object_id}#{subject_relation}"
        ));
        self.write(true, bearer_token, object_id, relation, &subject)
    }

    async fn query_object_owner(
        &self,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
    ) -> Result<(bool, String), ProviderError> {
        if self.hold_owner_read.swap(false, Ordering::SeqCst) {
            self.owner_read_waiting.set();
            self.owner_reads_released.wait().await;
        }
        self.reachable()?;
        Ok(match self.owners.get(object_id) {
            Some(owner) => (true, owner),
            None => (false, String::new()),
        })
    }

    async fn create_access_decision(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _permission: &str,
        _actor_did: &str,
    ) -> Result<Option<String>, ProviderError> {
        self.decisions.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    async fn verify_access(
        &self,
        _policy_id: &str,
        _resource: &str,
        object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> Result<bool, ProviderError> {
        self.reachable()?;
        if self.unchecked.contains_key(object_id) {
            return Err(ProviderError::Unavailable(format!(
                "stub provider cannot check {object_id}"
            )));
        }
        if self.stale.load(Ordering::SeqCst) {
            return Err(ProviderError::Unavailable(
                "verified permission: verified revision is stale; wait for a fresh finalized revision"
                    .into(),
            ));
        }
        let writes = self.owners.get(object_id).as_deref() == Some(actor_did)
            || self.holds_as(object_id, "writer", actor_did);
        Ok(match permission {
            "update" | "delete" => writes,
            _ => writes || self.holds(object_id, actor_did) || self.holds(object_id, "*"),
        })
    }
}

/// Wait until `holds` does, for at most 90 seconds: the node's drainer
/// submits what is owed by itself, after a back-off of up to 60 seconds.
pub(super) async fn eventually<F, Fut>(what: &str, mut holds: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while !holds().await {
        assert!(std::time::Instant::now() < deadline, "{what}");
        edge::testing::edge()
            .sleep(std::time::Duration::from_millis(100))
            .await;
    }
}

pub(super) fn actor(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

fn carrying(did: &str, token: &str) -> RequestContext {
    actor(did).with_bearer(Some(did.to_owned()), Some(token.to_owned()))
}

pub(super) fn node_identity() -> String {
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
pub(super) async fn start(
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

pub(super) async fn provision(node: &Node) {
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
        let in_own = owner.clone().with_txn(own.as_str().parse().ok());
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

        // The provider is down at the commit, and stays unable to take a
        // write on the document after it, so the node's drainer cannot
        // submit the revocation either: it holds on this node all the same,
        // and the provider still holds the grant.
        remote.cut_off(&doc, true);
        remote.down.store(true, Ordering::SeqCst);
        transaction.commit().await.unwrap();
        remote.down.store(false, Ordering::SeqCst);
        assert!(remote.holds(&doc, READER));
        assert!(reads(&node, READER).await.is_empty());
        assert!(remote.take_writes().is_empty());

        // A commit submits what is owed on the documents its own
        // transaction staged on, and is not held by what is owed elsewhere.
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
            [(true, other.clone(), READER.to_string())]
        );
        assert!(remote.holds(&doc, READER));
        assert_eq!(reads(&node, READER).await, ["other"]);

        // A change committed on the first document after the revocation
        // goes after it: once the provider takes writes there again, the
        // revocation is submitted first.
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.add_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &owner,
            "Record",
            &doc,
            "reader",
            CALLER,
        )
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        assert!(remote.take_writes().is_empty());
        remote.cut_off(&doc, false);
        eventually(
            "the drainer did not submit what is owed on the document",
            || async { remote.pending_writes() == 2 && remote.holds(&doc, CALLER) },
        )
        .await;
        assert_eq!(
            remote.take_writes(),
            [
                (false, doc.clone(), READER.to_string()),
                (true, doc.clone(), CALLER.to_string()),
            ]
        );
        assert!(!remote.holds(&doc, READER));
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

#[test]
fn a_revocation_is_not_staged_while_nobody_can_check_its_actor() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node_on(Some(&remote)).await;
        let doc = created(node.execute_request(create("shared")).await);
        node.add_dac_actor_relationship(OWNER, "Record", &doc, "reader", READER)
            .await
            .unwrap();
        remote.take_writes();
        assert_eq!(reads(&node, READER).await, ["shared"]);
        remote.down.store(true, Ordering::SeqCst);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let revoked = node
            .delete_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &actor(OWNER),
                "Record",
                &doc,
                "reader",
                "*",
            )
            .await;
        assert!(
            matches!(
                revoked,
                Err(crate::Error::Acp(acp::Error::OwedCheckUnavailable { .. }))
            ),
            "{revoked:?}"
        );
        transaction.commit().await.unwrap();
        remote.down.store(false, Ordering::SeqCst);

        // No record was written: a delete of everyone would deny the reader
        // from this commit on, and the next commit on the document would
        // submit it.
        assert_eq!(reads(&node, READER).await, ["shared"]);
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.add_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &actor(OWNER),
            "Record",
            &doc,
            "reader",
            CALLER,
        )
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(
            remote.take_writes(),
            [(true, doc.clone(), CALLER.to_string())]
        );
        assert_eq!(reads(&node, READER).await, ["shared"]);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A commit waits for the provider no longer than the deadline, and what
/// the provider did not take by then is still owed.
#[test]
fn a_commit_returns_at_the_deadline_with_the_change_still_staged() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node_on(Some(&remote)).await;
        let doc = created(node.execute_request(create("shared")).await);
        let deadline = crate::acp_ops::transactional::OWED_SETTLE_DEADLINE;

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.add_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &actor(OWNER),
            "Record",
            &doc,
            "reader",
            READER,
        )
        .await
        .unwrap();
        remote.stalled.store(true, Ordering::SeqCst);
        let committing = std::time::Instant::now();
        transaction.commit().await.unwrap();
        let waited = committing.elapsed();
        assert!(
            waited >= deadline && waited < deadline + std::time::Duration::from_secs(5),
            "the commit waited {waited:?}"
        );
        assert!(!remote.holds(&doc, READER));

        // The stalled submission never lands, and the provider takes no
        // write on the document until the next commit: the change is still
        // staged, and is submitted before the change committed after it.
        remote.cut_off(&doc, true);
        remote.stalled.store(false, Ordering::SeqCst);
        remote.released.set();
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.add_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &actor(OWNER),
            "Record",
            &doc,
            "reader",
            CALLER,
        )
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        assert!(!remote.holds(&doc, READER));
        remote.cut_off(&doc, false);
        eventually(
            "the drainer did not submit what is owed on the document",
            || async { remote.pending_writes() == 2 && remote.holds(&doc, CALLER) },
        )
        .await;
        assert_eq!(
            remote.take_writes(),
            [
                (true, doc.clone(), READER.to_string()),
                (true, doc.clone(), CALLER.to_string()),
            ]
        );
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A userset subject is submitted under the authority recorded with the
/// change: the provider applies the owner's and refuses a stranger's, though
/// the node could sign either as the owner.
#[test]
fn a_userset_grant_is_decided_under_its_own_actors_authority() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node_on(Some(&remote)).await;
        let doc = created(node.execute_request(create("shared")).await);
        let members = "groups:team#member";
        let stage = |context: RequestContext| {
            let (node, doc) = (&node, &doc);
            async move {
                let transaction = node.begin_transaction_guard(false).await.unwrap();
                let staged = node
                    .add_dac_actor_relationship_change_for_in_txn(
                        transaction.handle(),
                        &context,
                        "Record",
                        doc,
                        "reader",
                        members,
                    )
                    .await;
                transaction.commit().await.unwrap();
                staged
            }
        };

        // The caller owns nothing here. With the provider up it is refused
        // at staging.
        let delegation = format!("delegation:{CALLER}");
        assert!(matches!(
            stage(carrying(CALLER, &delegation)).await,
            Err(crate::Error::Acp(acp::Error::NotOwner { .. }))
        ));
        // With the provider down its add is staged unchecked, submitted
        // under its own delegation, refused by the provider and discarded.
        remote.down.store(true, Ordering::SeqCst);
        assert!(matches!(
            stage(carrying(CALLER, &delegation)).await,
            Ok(RelationshipChange::Staged {
                authority_checked: false,
                ..
            })
        ));
        remote.down.store(false, Ordering::SeqCst);
        // The node's drainer submits it once the provider is back; the
        // owner stages after that, so neither commit races the other.
        let caller = actor(CALLER);
        eventually("the drainer did not submit the caller's change", || async {
            node.owed_acp_as(&caller, CALLER)
                .await
                .unwrap()
                .iter()
                .all(|entry| entry.state != acp::OwedState::Staged)
        })
        .await;
        assert!(matches!(
            stage(actor(OWNER)).await,
            Ok(RelationshipChange::Staged {
                authority_checked: true,
                ..
            })
        ));

        // One write reached the provider's state: the owner's.
        assert_eq!(
            remote.take_writes(),
            [(true, doc.clone(), members.to_string())]
        );
        assert!(remote.holds(&doc, members));
        // Nothing of the caller's is left to be submitted again.
        remote
            .tuples
            .remove(&(doc.clone(), "reader".into(), members.into()));
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.add_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &actor(OWNER),
            "Record",
            &doc,
            "reader",
            READER,
        )
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(
            remote.take_writes(),
            [(true, doc.clone(), READER.to_string())]
        );
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A node whose store still owes changes in the layout before the owed
/// counts were split does not start: it would neither submit them nor deny
/// by an owed revocation among them. One that owes nothing there starts.
#[test]
fn a_node_owing_changes_in_the_earlier_layout_does_not_start() {
    use storage::corekv::Store as _;

    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node_did = node_identity();
        for (keys, refused) in [
            (
                vec![b"/acp-owed/seq".to_vec(), b"/acp-owed/n".to_vec()],
                false,
            ),
            (
                vec![[b"/acp-owed/q/".as_slice(), &3u64.to_be_bytes()].concat()],
                true,
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let first = start(&node_did, Some(&remote), Some(directory.path())).await;
            first.shutdown().await.unwrap();
            drop(first);
            {
                let store = Arc::new(storage::RegolithStore::open(directory.path()).unwrap());
                let acp = storage::namespace::NamespacedStore::new(
                    store,
                    storage::namespace::Namespace::Acpstore,
                );
                let mut txn = acp.new_txn(false).await.unwrap();
                for key in &keys {
                    txn.set(key, b"earlier").await.unwrap();
                }
                txn.commit().await.unwrap();
            }
            let second = Node::builder()
                .with_node_identity_did(&node_did)
                .with_document_acp_provider(DocumentAcpProvider::new(remote.clone()))
                .data_path(directory.path())
                .build()
                .await;
            match (second, refused) {
                (Err(crate::Error::Acp(acp::Error::LegacyOwedChanges { count: 1 })), true) => {}
                (Ok(node), false) => node.shutdown().await.unwrap(),
                (other, _) => panic!("{keys:?}: {:?}", other.map(|_| "started")),
            }
        }
    })
    .unwrap()
}
