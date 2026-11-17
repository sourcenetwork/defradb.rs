//! A remote provider several receivers share, a creator's node that signs
//! its own blocks, and receivers that merge them.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity, ReplicaGrant};
use async_trait::async_trait;
use blockstore::{Blockstore as _, DefraBlockstore};
use bytes::Bytes;
use cid::Cid;
use crypto::{Key as _, PrivateKey as _};
use db::database::{DbOptions, DB};
use db::merge::acp_merge_handler::AcpMergeHandler;
use db::merge::merge_handler::DbMergeHandler;
use db::merge::{ReplicaOwnership, ReplicaRegistration};
use db::AutoCommitMutator;
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use defra_core::signing::{set_signing_config, SigningConfig, SigningKeyType};
use document::{DocID, Document, NormalValue};
use identity::{Did, Identity as _};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use regolith::sync::HopscotchMap;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind, PolicyDescription};
use storage::RegolithStore;
use zanzibar::types::Subject;

pub(super) const POLICY: &str = "replica-policy";
pub(super) const RESOURCE: &str = "users";
pub(super) const COLLECTION_ID: &str = "col-users";

type Map<K, V> = HopscotchMap<K, V, rapidhash::fast::RandomState>;
type Blocks = DefraBlockstore<RegolithStore>;
pub(super) type Composite = (Cid, Bytes);

fn map<K, V>() -> Map<K, V>
where
    K: std::hash::Hash + Eq + Clone + 'static,
    V: Clone + 'static,
{
    HopscotchMap::with_hasher(rapidhash::fast::RandomState::default())
}

/// One Vera deployment, as far as these tests need one: owners, relations,
/// and switches that hold registrations or the adds of a relation, as an
/// unreachable provider would.
pub(super) struct Chain {
    owners: Map<String, Did>,
    relations: Map<(String, String, String), ()>,
    held_relations: Map<String, ()>,
    pub(super) hold_registrations: AtomicBool,
    pub(super) reads: AtomicUsize,
}

impl Chain {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            owners: map(),
            relations: map(),
            held_relations: map(),
            hold_registrations: AtomicBool::new(false),
            reads: AtomicUsize::new(0),
        })
    }

    pub(super) fn owner(&self, doc: &str) -> Option<Did> {
        self.owners.get(doc)
    }

    pub(super) fn holds(&self, doc: &str, relation: &str, did: &Did) -> bool {
        self.relations
            .contains_key(&(doc.to_owned(), relation.to_owned(), did.to_string()))
    }

    pub(super) fn grant(&self, doc: &str, relation: &str, did: &Did) {
        self.relations
            .insert((doc.to_owned(), relation.to_owned(), did.to_string()), ());
    }

    pub(super) fn revoke(&self, doc: &str, relation: &str, did: &Did) {
        self.relations
            .remove(&(doc.to_owned(), relation.to_owned(), did.to_string()));
    }

    /// Adds of `relation` fail as unreachable until released.
    pub(super) fn hold(&self, relation: &str) {
        self.held_relations.insert(relation.to_owned(), ());
    }

    pub(super) fn release(&self, relation: &str) {
        self.held_relations.remove(relation);
    }

    /// The deployment as one node sees it: it signs as `actor`.
    pub(super) fn as_node(self: &Arc<Self>, actor: &Did) -> Arc<dyn DocumentACP> {
        Arc::new(ChainView {
            chain: Arc::clone(self),
            actor: actor.clone(),
        })
    }
}

struct ChainView {
    chain: Arc<Chain>,
    actor: Did,
}

impl ChainView {
    fn read(&self) {
        self.chain.reads.fetch_add(1, Ordering::SeqCst);
    }

    fn change(
        &self,
        add: bool,
        requestor: &Did,
        target: &Did,
        doc: &str,
        relation: &str,
    ) -> acp::Result<bool> {
        if add && self.chain.held_relations.contains_key(relation) {
            return Err(acp::Error::ProviderUnavailable("held".into()));
        }
        if self.chain.owner(doc).as_ref() != Some(requestor) {
            return Err(acp::Error::NotOwner {
                operation: "create actor relationship".into(),
            });
        }
        if add {
            self.chain.grant(doc, relation, target);
        } else {
            self.chain.revoke(doc, relation, target);
        }
        Ok(true)
    }
}

#[async_trait]
impl DocumentACP for ChainView {
    fn unregistered_documents_are_public(&self) -> bool {
        false
    }

    fn own_actor(&self) -> Option<Did> {
        Some(self.actor.clone())
    }

    async fn register_doc_object(
        &self,
        identity: &Did,
        _: &str,
        _: &str,
        doc: &str,
    ) -> acp::Result<()> {
        if self.chain.hold_registrations.load(Ordering::SeqCst) {
            return Err(acp::Error::ProviderUnavailable("held".into()));
        }
        if self
            .chain
            .owners
            .insert_if_absent(doc.to_owned(), identity.clone())
            .is_some()
        {
            return Err(acp::Error::DocumentAlreadyRegistered(doc.to_owned()));
        }
        Ok(())
    }

    async fn is_doc_registered(&self, _: &str, _: &str, doc: &str) -> acp::Result<bool> {
        self.read();
        Ok(self.chain.owners.contains_key(doc))
    }

    async fn get_doc_owner(&self, _: &str, _: &str, doc: &str) -> acp::Result<Option<Did>> {
        self.read();
        Ok(self.chain.owner(doc))
    }

    async fn check_doc_access(
        &self,
        identity: &Identity,
        permission: DocumentPermission,
        _: &str,
        _: &str,
        doc: &str,
    ) -> acp::Result<bool> {
        self.read();
        let Identity::Authenticated(did) = identity else {
            return Ok(false);
        };
        if self.chain.owner(doc).as_ref() == Some(did) {
            return Ok(true);
        }
        let writes = self.chain.holds(doc, "writer", did);
        Ok(match permission {
            DocumentPermission::Read => writes || self.chain.holds(doc, "reader", did),
            _ => writes,
        })
    }

    async fn add_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        _: &str,
        _: &str,
        doc: &str,
        relation: &str,
        _: &[String],
    ) -> acp::Result<bool> {
        self.change(true, requestor, target, doc, relation)
    }

    async fn delete_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        _: &str,
        _: &str,
        doc: &str,
        relation: &str,
        _: &[String],
    ) -> acp::Result<bool> {
        self.change(false, requestor, target, doc, relation)
    }

    /// As vera.rs: a document delete never archives the object.
    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> acp::Result<()> {
        Ok(())
    }
}

/// The embedder: which actors are this deployment's own, and what a receiver
/// grants on a document it registers: read for the deployment's nodes, then
/// write for the creator.
pub(super) struct Embedder {
    pub(super) own: Vec<Did>,
    pub(super) nodes: Vec<Did>,
}

impl ReplicaOwnership for Embedder {
    fn is_own_actor(&self, _collection: &str, actor: &Did) -> bool {
        self.own.contains(actor)
    }

    fn creator_grants(&self, _collection: &str, creator: &Did, _doc_id: &str) -> Vec<ReplicaGrant> {
        self.nodes
            .iter()
            .map(|node| ReplicaGrant::new("reader", Subject::Entity(node.clone())))
            .chain([ReplicaGrant::new(
                "writer",
                Subject::Entity(creator.clone()),
            )])
            .collect()
    }
}

pub(super) fn users() -> CollectionVersion {
    CollectionVersion::new(
        "Users",
        "v1",
        COLLECTION_ID,
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "name", FieldKind::string(), CType::LwwRegister),
        ],
    )
    .with_policy(PolicyDescription::new(POLICY, RESOURCE))
}

/// A key that signs blocks, and its DID.
#[derive(Clone)]
pub(super) struct Key {
    signing: SigningConfig,
    pub(super) did: Did,
}

pub(super) fn key() -> Key {
    let key = crypto::generate_ed25519().unwrap();
    let public_key = key.public_key();
    Key {
        did: Did::new(public_key.did().unwrap()).unwrap(),
        signing: SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: key.raw_owned(),
            public_key_bytes: public_key.raw_owned(),
            public_key_hex: hex::encode(public_key.raw()),
            remote_signer: None,
            signing_authorization: None,
        },
    }
}

/// The creator's node, on its own deployment: no ACP, every block signed by
/// the key that writes it.
pub(super) struct Source {
    db: Arc<DB<RegolithStore>>,
    pub(super) blocks: Arc<Blocks>,
}

impl Source {
    pub(super) async fn open() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(users()).await.unwrap();
        Self {
            db,
            blocks: Arc::new(DefraBlockstore::new(store, false)),
        }
    }

    fn document(&self) -> Document {
        Document::new(
            self.db
                .get_collection("Users")
                .unwrap()
                .unwrap()
                .schema_arc(),
        )
    }

    pub(super) async fn create(&self, by: &Key, name: &str) -> (String, Composite) {
        let mut doc = self.document();
        doc.set("name", NormalValue::String(name.into())).unwrap();
        set_signing_config(Some(by.signing.clone()));
        let created = AutoCommitMutator::new(self.db.clone())
            .create("Users", doc)
            .await;
        set_signing_config(None);
        let created = created.unwrap();
        (
            created.doc_id.to_string(),
            (created.commit_cid.unwrap(), created.commit_block.unwrap()),
        )
    }

    pub(super) async fn update(&self, by: &Key, doc_id: &str, name: &str) -> Composite {
        let mut doc = self.document();
        doc.set_id(DocID::from_string(doc_id).unwrap());
        doc.set("name", NormalValue::String(name.into())).unwrap();
        set_signing_config(Some(by.signing.clone()));
        let updated = AutoCommitMutator::new(self.db.clone())
            .update(
                "Users",
                doc,
                ["name".to_owned()].into_iter().collect::<RapidHashSet<_>>(),
            )
            .await;
        set_signing_config(None);
        let updated = updated.unwrap();
        (updated.commit_cid.unwrap(), updated.commit_block.unwrap())
    }

    pub(super) async fn delete(&self, by: &Key, doc_id: &str) -> Composite {
        set_signing_config(Some(by.signing.clone()));
        let deleted = AutoCommitMutator::new(self.db.clone())
            .delete("Users", &DocID::from_string(doc_id).unwrap())
            .await;
        set_signing_config(None);
        let deleted = deleted.unwrap();
        (deleted.commit_cid.unwrap(), deleted.commit_block.unwrap())
    }
}

/// A receiving node of the shared deployment, signing as `actor`, under an
/// identity of its own.
pub(super) struct Receiver {
    handler: AcpMergeHandler<RegolithStore, Blocks>,
    blocks: Arc<Blocks>,
    pub(super) db: Arc<DB<RegolithStore>>,
    pub(super) identity: Did,
}

impl Receiver {
    pub(super) async fn open(
        chain: &Arc<Chain>,
        actor: &Did,
        replicas: Option<Arc<dyn ReplicaOwnership>>,
    ) -> Self {
        let acp = chain.as_node(actor);
        Self::open_with(move |_| acp, replicas, true).await
    }

    /// A node whose document ACP `acp` builds over its store; `strict` is
    /// the access a remote provider runs under.
    pub(super) async fn open_with(
        acp: impl FnOnce(Arc<RegolithStore>) -> Arc<dyn DocumentACP>,
        replicas: Option<Arc<dyn ReplicaOwnership>>,
        strict: bool,
    ) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let acp = acp(store.clone());
        let node =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let identity = node.did().unwrap();
        let db = Arc::new(
            DB::from_arc_with_options(store.clone(), DbOptions::new().with_node_identity(node))
                .unwrap(),
        );
        db.create_collection(users()).await.unwrap();
        if let Some(ownership) = replicas {
            db.set_replica_registration(ReplicaRegistration::new(["Users"], ownership));
        }
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let handler =
            AcpMergeHandler::new(Arc::new(DbMergeHandler::new(db.clone(), blocks.clone())))
                .with_document_acp(acp);
        handler.set_strict_replicated_doc_access(strict);
        Self {
            handler,
            blocks,
            db,
            identity,
        }
    }

    /// Copy `composite` and everything it reaches from `from`, then merge it.
    pub(super) async fn receive(
        &self,
        from: &Source,
        doc_id: &str,
        composite: &Composite,
    ) -> MergeOutcome {
        let mut pending = vec![composite.0];
        while let Some(cid) = pending.pop() {
            if self.blocks.get(&cid).await.unwrap().is_some() {
                continue;
            }
            let Some(data) = from.blocks.get(&cid).await.unwrap() else {
                continue;
            };
            if let Ok(block) = Block::from_dag_cbor(&data) {
                pending.extend(block.signature);
                pending.extend(block.links.iter().flatten().map(|link| link.link));
                pending.extend(block.heads.iter().flatten().copied());
            }
            self.blocks.put(&cid, &data).await.unwrap();
        }
        self.handler
            .handle_block(
                &composite.0,
                &composite.1,
                BlockMetadata::normal(
                    doc_id,
                    COLLECTION_ID,
                    "did:key:z6MkCarrier",
                    Some("peer"),
                    false,
                ),
            )
            .await
            .unwrap()
    }

    pub(super) fn acp(&self) -> Arc<dyn DocumentACP> {
        self.db.document_acp().unwrap().clone()
    }

    /// Submit what this node owes on `doc` as a replica.
    pub(super) async fn settle(&self, doc: &str) -> acp::Result<()> {
        self.acp().settle_replica(POLICY, RESOURCE, doc).await
    }

    pub(super) async fn owed(&self, doc: &str) -> acp::ReplicaOwed {
        self.acp()
            .replica_owed(POLICY, RESOURCE, doc)
            .await
            .unwrap()
    }
}

pub(super) fn is_retry(outcome: &MergeOutcome) -> bool {
    matches!(
        outcome,
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    )
}
