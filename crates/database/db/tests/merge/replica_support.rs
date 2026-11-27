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
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, Signature, SignatureHeader, SignatureType,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use defra_core::signing::{set_signing_config, SigningConfig, SigningKeyType};
use document::{DocID, Document, NormalValue};
use identity::{Did, Identity as _};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use regolith::sync::HopscotchMap;
use schema::CollectionVersion;
use storage::RegolithStore;
use zanzibar::types::Subject;

pub(crate) const POLICY: &str = "replica-policy";
pub(crate) const RESOURCE: &str = "users";
pub(crate) const USERS_SDL: &str =
    r#"type Users @policy(id: "replica-policy", resource: "users") { name: String }"#;

type Map<K, V> = HopscotchMap<K, V, rapidhash::fast::RandomState>;
type Blocks = DefraBlockstore<RegolithStore>;
pub(crate) type Composite = (Cid, Bytes);

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
pub(crate) struct Chain {
    owners: Map<String, Did>,
    relations: Map<(String, String, String), ()>,
    held_relations: Map<String, ()>,
    pub(crate) hold_registrations: AtomicBool,
    pub(crate) reads: AtomicUsize,
}

impl Chain {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            owners: map(),
            relations: map(),
            held_relations: map(),
            hold_registrations: AtomicBool::new(false),
            reads: AtomicUsize::new(0),
        })
    }

    pub(crate) fn owner(&self, doc: &str) -> Option<Did> {
        self.owners.get(doc)
    }

    pub(crate) fn holds(&self, doc: &str, relation: &str, did: &Did) -> bool {
        self.relations
            .contains_key(&(doc.to_owned(), relation.to_owned(), did.to_string()))
    }

    pub(crate) fn grant(&self, doc: &str, relation: &str, did: &Did) {
        self.relations
            .insert((doc.to_owned(), relation.to_owned(), did.to_string()), ());
    }

    pub(crate) fn revoke(&self, doc: &str, relation: &str, did: &Did) {
        self.relations
            .remove(&(doc.to_owned(), relation.to_owned(), did.to_string()));
    }

    /// Adds of `relation` fail as unreachable until released.
    pub(crate) fn hold(&self, relation: &str) {
        self.held_relations.insert(relation.to_owned(), ());
    }

    pub(crate) fn release(&self, relation: &str) {
        self.held_relations.remove(relation);
    }

    /// The deployment as one node sees it: it signs as `actor`.
    pub(crate) fn as_node(self: &Arc<Self>, actor: &Did) -> Arc<dyn DocumentACP> {
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
        _relationships: &[acp::DerivedRelationship],
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
pub(crate) struct Embedder {
    pub(crate) own: Vec<Did>,
    pub(crate) nodes: Vec<Did>,
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

/// A key that signs blocks, and its DID.
#[derive(Clone)]
pub(crate) struct Key {
    signing: SigningConfig,
    private: Arc<crypto::Ed25519PrivateKey>,
    pub(crate) did: Did,
}

pub(crate) fn key() -> Key {
    let key = crypto::generate_ed25519().unwrap();
    let public_key = key.public_key();
    Key {
        private: Arc::new(key.clone()),
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
pub(crate) struct Source {
    db: Arc<DB<RegolithStore>>,
    pub(crate) blocks: Arc<Blocks>,
    version: CollectionVersion,
}

impl Source {
    pub(crate) async fn open() -> Self {
        Self::with_sdl(USERS_SDL).await
    }

    /// A creator's node holding the `Users` that `sdl` defines.
    pub(crate) async fn with_sdl(sdl: &str) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        let version = db
            .add_collection(&defra_core::RequestContext::ambient(), sdl)
            .await
            .unwrap()
            .remove(0);
        Self {
            db,
            blocks: Arc::new(DefraBlockstore::new(store, false)),
            version,
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

    /// `by` creates a document named `name`.
    pub(crate) async fn create(&self, by: &Key, name: &str) -> (String, Composite) {
        let mut doc = self.document();
        doc.set("name", NormalValue::String(name.into())).unwrap();
        set_signing_config(Some(by.signing.clone()));
        let created = defra_core::signing::scope_broadcast_creator_did(
            None,
            AutoCommitMutator::new(self.db.clone()).create("Users", doc),
        )
        .await;
        set_signing_config(None);
        let created = created.unwrap();
        (
            created.doc_id.to_string(),
            (created.commit_cid.unwrap(), created.commit_block.unwrap()),
        )
    }

    pub(crate) async fn update(&self, by: &Key, doc_id: &str, name: &str) -> Composite {
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

    /// A composite with no field changes on top of `heads`, signed by `by`:
    /// a change on whichever branch `heads` names.
    pub(crate) async fn composite(
        &self,
        by: &Key,
        heads: Vec<Cid>,
        priority: u64,
        status: u8,
    ) -> Composite {
        let mut block = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: self.version.version_id.clone(),
                priority,
                status,
            }),
            heads,
            vec![],
        );
        let value = by.private.sign(&block.to_dag_cbor().unwrap()).unwrap();
        let signature = Signature::new(
            SignatureHeader::new(
                SignatureType::EdDSA,
                hex::encode(by.private.public_key().raw()).into_bytes(),
            ),
            value,
        );
        let signature_cid = signature.generate_cid().unwrap();
        self.blocks
            .put(&signature_cid, &signature.to_dag_cbor().unwrap())
            .await
            .unwrap();
        block.signature = Some(signature_cid);
        let cid = block.generate_cid().unwrap();
        let bytes = Bytes::from(block.to_dag_cbor().unwrap());
        self.blocks.put(&cid, &bytes).await.unwrap();
        (cid, bytes)
    }

    /// An unsigned composite on top of `heads`.
    pub(crate) async fn unsigned(&self, heads: Vec<Cid>, priority: u64) -> Composite {
        let block = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: self.version.version_id.clone(),
                priority,
                status: 1,
            }),
            heads,
            vec![],
        );
        let cid = block.generate_cid().unwrap();
        let bytes = Bytes::from(block.to_dag_cbor().unwrap());
        self.blocks.put(&cid, &bytes).await.unwrap();
        (cid, bytes)
    }

    pub(crate) async fn delete(&self, by: &Key, doc_id: &str) -> Composite {
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
pub(crate) struct Receiver {
    collection_id: String,
    handler: AcpMergeHandler<RegolithStore, Blocks>,
    blocks: Arc<Blocks>,
    pub(crate) db: Arc<DB<RegolithStore>>,
    pub(crate) identity: Did,
}

impl Receiver {
    pub(crate) async fn open(
        chain: &Arc<Chain>,
        actor: &Did,
        replicas: Option<Arc<dyn ReplicaOwnership>>,
    ) -> Self {
        let acp = chain.as_node(actor);
        Self::open_with(move |_| acp, replicas, true).await
    }

    /// A node whose document ACP `acp` builds over its store; `strict` is
    /// the access a remote provider runs under.
    pub(crate) async fn open_with(
        acp: impl FnOnce(Arc<RegolithStore>) -> Arc<dyn DocumentACP>,
        replicas: Option<Arc<dyn ReplicaOwnership>>,
        strict: bool,
    ) -> Self {
        Self::open_collection(USERS_SDL, acp, replicas, strict).await
    }

    /// A node holding the `Users` that `sdl` defines.
    pub(crate) async fn open_collection(
        sdl: &str,
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
        let collection_id = db
            .add_collection(&defra_core::RequestContext::ambient(), sdl)
            .await
            .unwrap()
            .remove(0)
            .collection_id;
        if let Some(ownership) = replicas {
            db.set_replica_registration(ReplicaRegistration::new(["Users"], ownership));
        }
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let handler =
            AcpMergeHandler::new(Arc::new(DbMergeHandler::new(db.clone(), blocks.clone())))
                .with_document_acp(acp);
        handler.set_strict_replicated_doc_access(strict);
        Self {
            collection_id,
            handler,
            blocks,
            db,
            identity,
        }
    }

    /// Copy `composite` and everything it reaches from `from`, then merge it.
    pub(crate) async fn receive(
        &self,
        from: &Source,
        doc_id: &str,
        composite: &Composite,
    ) -> MergeOutcome {
        self.try_receive(from, doc_id, composite).await.unwrap()
    }

    pub(crate) async fn try_receive(
        &self,
        from: &Source,
        doc_id: &str,
        composite: &Composite,
    ) -> Result<MergeOutcome, db::merge::merge_handler::MergeError> {
        self.copy_blocks(from, composite).await;
        self.handler
            .handle_block(
                &composite.0,
                &composite.1,
                BlockMetadata::normal(
                    doc_id,
                    &self.collection_id,
                    "did:key:z6MkCarrier",
                    Some("peer"),
                    false,
                ),
            )
            .await
    }

    async fn copy_blocks(&self, from: &Source, composite: &Composite) {
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
    }

    /// Re-drive what is ready to be tried again.
    pub(crate) async fn redrive(&self) {
        self.handler.inner().redrive_deferred().await;
    }

    /// One pass of the bounded sweep over parked changes.
    pub(crate) async fn sweep_parked(&self) -> usize {
        self.handler.inner().sweep_parked_acp().await
    }

    pub(crate) fn acp(&self) -> Arc<dyn DocumentACP> {
        self.db.document_acp().unwrap().clone()
    }

    /// Submit what this node owes on `doc` as a replica.
    pub(crate) async fn settle(&self, doc: &str) -> acp::Result<()> {
        self.acp().settle_replica(POLICY, RESOURCE, doc).await
    }

    pub(crate) async fn owed(&self, doc: &str) -> acp::ReplicaOwed {
        self.acp()
            .replica_owed(POLICY, RESOURCE, doc)
            .await
            .unwrap()
    }
}

pub(crate) fn is_parked(outcome: &MergeOutcome) -> bool {
    matches!(outcome, MergeOutcome::Skipped { terminal: false, reason } if reason.starts_with("parked: "))
}

pub(crate) fn is_retry(outcome: &MergeOutcome) -> bool {
    matches!(
        outcome,
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    )
}
