//! Authorization must survive bounded composite-history traversal and retries.

use acp::{DocumentACP, LocalDocumentACP, MemoryAcpStore};
use async_trait::async_trait;
use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use crypto::PrivateKey as _;
use db::database::DB;
use db::merge::acp_merge_handler::AcpCompositeMergeHook;
use db::merge::merge_handler::hook::{CompositeFrame, CompositeMergeHook};
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, Signature, SignatureHeader, SignatureType,
};
use defra_core::merge::{
    BlockMetadata, ExplicitReplayAuthorization, MergeBlock, MergeHandler, MergeOutcome,
};
use document::{Document, NormalValue};
use identity::Did;
use schema::{CollectionVersion, FieldDescription, FieldKind, PolicyDescription};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use storage::RegolithStore;

type Blocks = DefraBlockstore<RegolithStore>;

struct Signer {
    key: crypto::Ed25519PrivateKey,
    did: String,
}

fn signer() -> Signer {
    let key = crypto::generate_ed25519().unwrap();
    let did = key.public_key().did().unwrap();
    Signer { key, did }
}

struct Fixture {
    handler: DbMergeHandler<RegolithStore, Blocks>,
    blockstore: Arc<Blocks>,
    doc_id: String,
    genesis: Cid,
    acp: Option<Arc<LocalDocumentACP>>,
}

impl Fixture {
    async fn new(owner: &Signer, hook: Option<Arc<dyn CompositeMergeHook>>) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(
            CollectionVersion::new(
                "Users",
                "v1",
                "col-users",
                vec![
                    FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                    FieldDescription::new("2", "name", FieldKind::string()),
                ],
            )
            .with_policy(PolicyDescription::new("policy-1", "users")),
        )
        .await
        .unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new_with_max_merge_depth(db, blockstore.clone(), 8);
        let mut doc = Document::new();
        doc.set("name", NormalValue::String("Alice".into()));
        let genesis = db::block::builder::build_blocks_from_document(&doc, "v1", &blockstore)
            .await
            .unwrap();
        assert_eq!(
            handler
                .handle_block(
                    &genesis.cid,
                    &genesis.block,
                    BlockMetadata::normal(&genesis.doc_id, "col-users", &owner.did, None, false),
                )
                .await
                .unwrap(),
            MergeOutcome::Merged
        );
        let mut document_acp = None;
        let hook = match hook {
            Some(hook) => hook,
            None => {
                let acp = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
                acp.register_doc_object(
                    &Did::new(owner.did.clone()).unwrap(),
                    "policy-1",
                    "users",
                    &genesis.doc_id,
                )
                .await
                .unwrap();
                let hook = Arc::new(AcpCompositeMergeHook::new(None));
                hook.set_document_acp(acp.clone());
                document_acp = Some(acp);
                hook
            }
        };
        handler.set_composite_merge_hook(hook);
        Self {
            handler,
            blockstore,
            doc_id: genesis.doc_id,
            genesis: genesis.cid,
            acp: document_acp,
        }
    }

    async fn update(&self, parent: Cid, priority: u64, signer: &Signer) -> (Cid, Vec<u8>) {
        let mut block = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                schema_version_id: "v1".into(),
                priority,
                status: 1,
            }),
            vec![parent],
            vec![],
        );
        let signature = Signature::new(
            SignatureHeader::new(
                SignatureType::EdDSA,
                hex::encode(signer.key.public_key().raw()).into_bytes(),
            ),
            signer.key.sign(&block.to_dag_cbor().unwrap()).unwrap(),
        );
        let signature_cid = signature.generate_cid().unwrap();
        self.blockstore
            .put(&signature_cid, &signature.to_dag_cbor().unwrap())
            .await
            .unwrap();
        block.signature = Some(signature_cid);
        let cid = block.generate_cid().unwrap();
        let bytes = block.to_dag_cbor().unwrap();
        self.blockstore.put(&cid, &bytes).await.unwrap();
        (cid, bytes)
    }

    async fn history(&self, owner: &Signer, attacker: Option<&Signer>, length: u64) -> MergeBlock {
        let mut cid = self.genesis;
        let mut bytes = Vec::new();
        for priority in 2..=length + 1 {
            // The bad frame is older than several traversal budgets, not the root.
            let signer = if priority == 3 {
                attacker.unwrap_or(owner)
            } else {
                owner
            };
            (cid, bytes) = self.update(cid, priority, signer).await;
        }
        MergeBlock {
            cid,
            block_data: bytes.into(),
            doc_id: self.doc_id.clone(),
            collection_id: "col-users".into(),
            creator: owner.did.clone(),
            sender_peer: Some("peer".into()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }
    }

    async fn attempt(&self, root: &MergeBlock, batch: bool) -> MergeOutcome {
        if batch {
            let mut results = self
                .handler
                .handle_block_batch(std::slice::from_ref(root))
                .await;
            assert_eq!(results.len(), 1);
            results
                .remove(0)
                .expect("history traversal must yield rather than error")
        } else {
            // Standalone delivery validates replay authorization separately from handle_block.
            self.handler
                .validate_authorization(root.explicit_replay_authorization.as_ref(), root)
                .await
                .expect("replay authorization must match the root's own signer");
            let metadata = BlockMetadata::normal(
                &root.doc_id,
                &root.collection_id,
                &root.creator,
                root.sender_peer.as_deref(),
                root.is_explicit_replicator,
            )
            .with_explicit_replay_authorization(root.explicit_replay_authorization.clone());
            self.handler
                .handle_block(&root.cid, &root.block_data, metadata)
                .await
                .expect("history traversal must yield rather than error")
        }
    }

    async fn finish(&self, root: &MergeBlock, batch: bool) -> (MergeOutcome, usize) {
        for retries in 0..128 {
            let outcome = self.attempt(root, batch).await;
            if !matches!(outcome, MergeOutcome::Yielded) {
                return (outcome, retries);
            }
        }
        panic!("history made no bounded progress after 128 attempts");
    }
}

async fn authorized_history(batch: bool) {
    let owner = signer();
    let fixture = Fixture::new(&owner, None).await;
    let root = fixture.history(&owner, None, 32).await;
    let (outcome, retries) = fixture.finish(&root, batch).await;
    assert!(retries > 0, "small depth budget must yield");
    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(fixture.attempt(&root, batch).await.is_terminal_skip());
}

async fn unauthorized_ancestor(batch: bool, explicit: bool) {
    let owner = signer();
    let attacker = signer();
    let fixture = Fixture::new(&owner, None).await;
    let mut root = fixture.history(&owner, Some(&attacker), 64).await;
    if explicit {
        authorize_replay(&mut root, &owner);
    }
    let (outcome, retries) = fixture.finish(&root, batch).await;
    assert!(
        retries > 0,
        "deep unauthorized frame must require resumption"
    );
    let denied = MergeOutcome::rejected(format!(
        "signer {} lacks update permission on protected document {}",
        attacker.did, fixture.doc_id
    ));
    assert_eq!(outcome, denied);
    // Neither rejection nor partial progress may mark the authorized root merged.
    assert_eq!(fixture.finish(&root, batch).await.0, denied);
}

fn authorize_replay(root: &mut MergeBlock, owner: &Signer) {
    root.is_explicit_replicator = true;
    root.explicit_replay_authorization = Some(ExplicitReplayAuthorization {
        source_peer_id: "peer".into(),
        target_peer_id: "receiver".into(),
        collection_id: "col-users".into(),
        authorizer_did: owner.did.clone(),
        expires_at: u64::MAX,
        capability: None,
    });
}

async fn authorized_ancestor_with_distinct_replay_signer(batch: bool) {
    let owner = signer();
    let updater = signer();
    let fixture = Fixture::new(&owner, None).await;
    assert!(fixture
        .acp
        .as_ref()
        .unwrap()
        .add_actor_relationship(
            &Did::new(owner.did.clone()).unwrap(),
            &Did::new(updater.did.clone()).unwrap(),
            "policy-1",
            "users",
            &fixture.doc_id,
            "updater",
            &[],
        )
        .await
        .unwrap());
    let mut root = fixture.history(&updater, None, 32).await;
    let (cid, bytes) = fixture.update(root.cid, 34, &owner).await;
    root.cid = cid;
    root.block_data = bytes.into();
    root.creator = owner.did.clone();
    authorize_replay(&mut root, &owner);

    // Replay authorization belongs to the carrier, not its differently signed ancestors.
    let (outcome, retries) = fixture.finish(&root, batch).await;
    assert!(retries > 0, "small depth budget must yield");
    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(fixture.attempt(&root, batch).await.is_terminal_skip());
}

#[derive(Default)]
struct RevocableHook {
    revoked: AtomicBool,
    checks: AtomicUsize,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl CompositeMergeHook for RevocableHook {
    fn guards_protected_updates(&self) -> bool {
        true
    }

    async fn on_protected_update(
        &self,
        _doc_id: &str,
        _collection: &CollectionVersion,
        frame: CompositeFrame<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        if frame.is_genesis {
            return Ok(None);
        }
        assert!(
            frame.signer.is_some(),
            "each update must retain its verified signer"
        );
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .revoked
            .load(Ordering::SeqCst)
            .then(|| MergeOutcome::rejected("update permission revoked")))
    }
}

async fn revoked_after_yield(batch: bool) {
    let owner = signer();
    let hook = Arc::new(RevocableHook::default());
    let fixture = Fixture::new(&owner, Some(hook.clone())).await;
    let root = fixture.history(&owner, None, 32).await;
    let first = fixture.attempt(&root, batch).await;
    assert!(
        matches!(first, MergeOutcome::Yielded),
        "expected a resumable yield, got {first:?}"
    );
    let checks = hook.checks.load(Ordering::SeqCst);
    hook.revoked.store(true, Ordering::SeqCst);
    assert_eq!(
        fixture.finish(&root, batch).await.0,
        MergeOutcome::rejected("update permission revoked")
    );
    assert!(hook.checks.load(Ordering::SeqCst) > checks);
}

#[tokio::test]
async fn standalone_authorized_history_resumes() {
    authorized_history(false).await;
}

#[tokio::test]
async fn batch_authorized_history_resumes() {
    authorized_history(true).await;
}

#[tokio::test]
async fn standalone_deep_unauthorized_ancestor_is_rejected() {
    unauthorized_ancestor(false, false).await;
}

#[tokio::test]
async fn batch_deep_unauthorized_ancestor_is_rejected() {
    unauthorized_ancestor(true, false).await;
}

#[tokio::test]
async fn standalone_explicit_replay_preserves_ancestor_signer() {
    unauthorized_ancestor(false, true).await;
}

#[tokio::test]
async fn batch_explicit_replay_preserves_ancestor_signer() {
    unauthorized_ancestor(true, true).await;
}

#[tokio::test]
async fn standalone_replay_accepts_distinct_authorized_ancestor_signer() {
    authorized_ancestor_with_distinct_replay_signer(false).await;
}

#[tokio::test]
async fn batch_replay_accepts_distinct_authorized_ancestor_signer() {
    authorized_ancestor_with_distinct_replay_signer(true).await;
}

#[tokio::test]
async fn standalone_resumption_rechecks_revoked_permission() {
    revoked_after_yield(false).await;
}

#[tokio::test]
async fn batch_resumption_rechecks_revoked_permission() {
    revoked_after_yield(true).await;
}
