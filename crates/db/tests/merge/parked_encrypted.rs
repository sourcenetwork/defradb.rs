//! A node without a causal custom type parks an encrypted document's blocks
//! of it. Built again with the type, it replays them with the keys it holds
//! itself: a block whose history decrypts with local keys merges, one whose
//! history needs a key held elsewhere stays parked, and no peer is asked.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use blockstore::{Blockstore as _, DefraBlockstore};
use bytes::Bytes;
use cid::Cid;
use crdt::field::TextField;
use crdt::stored::{Part, StoredField, ValueKind};
use crdt::{CausalCrdt, Crdts, DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, MergeResult};
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use db::{AutoCommitMutator, LensedAutoCommitFetcher};
use defra_core::block::{Block, CrdtDelta, CustomDeltaPayload};
use defra_core::encryption::{set_encryption_config, EncryptionConfig};
use defra_core::merge::{BlockMetadata, MergeHandler};
use defra_core::{Encryption, TextOp};
use document::{DocID, Document, FieldOp, NormalValue};
use kovan_map::HopscotchMap;
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use rapidhash::fast::RandomState;
use rapidhash::{HashSetExt, RapidHashSet};
use schema::{CType, CollectionVersion, CrdtName, FieldDescription, FieldKind};
use storage::{RegolithStore, RegolithStoreOptions};

const NAME: &str = "ctext";
const COLLECTION_ID: &str = "col-notes";

fn name() -> CrdtName {
    CrdtName::new(NAME).unwrap()
}

/// Text under a custom name: a causal type, whose merge of a concurrent block
/// reads the blocks below it.
struct CustomText;

impl FieldCrdt for CustomText {
    fn ctype(&self) -> CType {
        CType::Custom(name())
    }

    fn replicates_ops(&self) -> bool {
        TextField.replicates_ops()
    }

    fn op(&self, input: NormalValue, creating: bool) -> Result<FieldOp, FieldCrdtError> {
        TextField.op(input, creating)
    }

    fn named_op(&self, op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        TextField.named_op(op)
    }

    fn apply(
        &self,
        field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        TextField.apply(field, current, op)
    }

    fn write<'value>(
        &self,
        value: &'value NormalValue,
        op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value> {
        TextField.write(value, op)
    }

    fn block_data(
        &self,
        value: &NormalValue,
        op: Option<&FieldOp>,
    ) -> Result<Option<Vec<u8>>, FieldCrdtError> {
        TextField.block_data(value, op)
    }

    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        TextField.encode(data)
    }

    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        TextField.decode(field, data)
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Custom(CustomDeltaPayload {
            crdt: NAME.to_owned(),
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            nonce: None,
            collection_version_id: parts.collection_version_id.to_owned(),
            data: parts.data,
        })
    }

    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        let mut state = TextField.state(field, priority, value)?;
        state.kind = ValueKind::Custom(name());
        Ok(state)
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        (state.kind == ValueKind::Custom(name()))
            .then(|| {
                TextField.materialize(Part {
                    kind: ValueKind::Text,
                    ..state
                })
            })
            .flatten()
    }

    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        TextField.merge(state, incoming)
    }

    fn folds_exactly(&self, state: Part<'_>) -> bool {
        TextField.folds_exactly(state)
    }

    fn causal(&self) -> Option<&dyn CausalCrdt> {
        Some(&TextField)
    }
}

/// Every key any node generated, as the network would serve it.
#[derive(Default)]
struct Vault {
    keys: HopscotchMap<Cid, [u8; 32], RandomState>,
    made: AtomicUsize,
}

/// One node's KMS: the keys it holds itself (`None`: all of them), the rest
/// from its peers, counting every ask that may reach them.
struct Kms {
    vault: Arc<Vault>,
    held: Option<RapidHashSet<Cid>>,
    asked: AtomicUsize,
}

impl Kms {
    fn new(vault: &Arc<Vault>, held: Option<RapidHashSet<Cid>>) -> Arc<Self> {
        Arc::new(Self {
            vault: vault.clone(),
            held,
            asked: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl kms::KmsService for Kms {
    async fn generate_key(
        &self,
        _: &kms::RequestContext,
        _: kms::KeyScope,
    ) -> kms::Result<(kms::EncryptionCid, [u8; 32])> {
        let made = self.vault.made.fetch_add(1, Ordering::SeqCst) + 1;
        let mut key = [0; 32];
        key[..8].copy_from_slice(&made.to_be_bytes());
        let block = Encryption { key: key.to_vec() };
        let cid =
            defra_core::block::generate_cid_from_bytes(&block.to_dag_cbor().unwrap()).unwrap();
        self.vault.keys.insert(cid, key);
        Ok((cid, key))
    }

    async fn get_keys(
        &self,
        _: &kms::RequestContext,
        cids: &[kms::EncryptionCid],
    ) -> kms::Result<kms::KeyResults> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let (results, sender) = kms::KeyResults::new(cids.len());
        for cid in cids {
            let key = self.vault.keys.get(cid).ok_or(kms::Error::KeyUnavailable)?;
            sender.send(Ok((*cid, key))).await.unwrap();
        }
        drop(sender);
        Ok(results)
    }

    async fn get_local_keys_each(
        &self,
        _: &kms::RequestContext,
        cids: &[kms::EncryptionCid],
    ) -> kms::Result<Vec<kms::Result<[u8; 32]>>> {
        Ok(cids
            .iter()
            .map(|cid| {
                self.held
                    .as_ref()
                    .is_none_or(|held| held.contains(cid))
                    .then(|| self.vault.keys.get(cid))
                    .flatten()
                    .ok_or(kms::Error::KeyUnavailable)
            })
            .collect())
    }

    async fn serve_request(
        &self,
        _: kms::PeerIdentity,
        _: kms::FetchEncryptionKeyRequest,
    ) -> kms::Result<kms::FetchEncryptionKeyReply> {
        Err(kms::Error::Unsupported("no peer requests"))
    }
}

struct Node {
    db: Arc<DB<RegolithStore>>,
    blocks: Arc<DefraBlockstore<RegolithStore>>,
    handler: DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
    kms: Arc<Kms>,
}

fn with_type() -> Crdts {
    Crdts::new([crdt::Registration::new(CustomText)]).unwrap()
}

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        "Notes",
        "v1",
        COLLECTION_ID,
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "body", FieldKind::string(), CType::Custom(name())),
            FieldDescription::new("3", "title", FieldKind::string(), CType::LwwRegister),
        ],
    )
}

impl Node {
    async fn open(store: RegolithStore, kms: Arc<Kms>, fresh: bool) -> Self {
        let store = Arc::new(store);
        let db = if fresh {
            let db = Arc::new(DB::from_arc(store.clone()).unwrap());
            db.create_collection(collection()).await.unwrap();
            db
        } else {
            Arc::new(DB::open_from_arc(store.clone()).await.unwrap())
        };
        db.set_kms(kms.clone());
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(db.clone(), blocks.clone());
        handler.set_kms(kms.clone());
        Self {
            db,
            blocks,
            handler,
            kms,
        }
    }

    async fn in_memory(vault: &Arc<Vault>) -> Self {
        let options = RegolithStoreOptions::memory().with_crdts(with_type());
        let store = RegolithStore::open_with_options("regolith-memory", options).unwrap();
        Self::open(store, Kms::new(vault, None), true).await
    }

    /// The node on disk at `path`, with the type or without it.
    async fn on_disk(path: &Path, typed: bool, kms: Arc<Kms>, fresh: bool) -> Self {
        let options = RegolithStoreOptions::default();
        let options = if typed {
            options.with_crdts(with_type())
        } else {
            options
        };
        let store = RegolithStore::open_with_options(path, options).unwrap();
        Self::open(store, kms, fresh).await
    }

    async fn close(self) {
        self.db.close().await.unwrap();
    }

    fn document(&self, doc_id: Option<&DocID>) -> Document {
        let mut doc = Document::new(
            self.db
                .get_collection("Notes")
                .unwrap()
                .unwrap()
                .schema_arc(),
        );
        if let Some(doc_id) = doc_id {
            doc.set_id(doc_id.clone());
        }
        doc
    }

    async fn create(&self, body: &str) -> DocID {
        let mut doc = self.document(None);
        doc.set("body", text(body)).unwrap();
        doc.set("title", text(body)).unwrap();
        AutoCommitMutator::new(self.db.clone())
            .create("Notes", doc)
            .await
            .unwrap()
            .doc_id
    }

    async fn update(&self, doc: Document) -> (Cid, Bytes) {
        let result = AutoCommitMutator::new(self.db.clone())
            .update("Notes", doc, ["body".to_owned()].into_iter().collect())
            .await
            .unwrap();
        (result.commit_cid.unwrap(), result.commit_block.unwrap())
    }

    async fn heads(&self, doc_id: &DocID) -> Vec<(Cid, Bytes)> {
        db::merge::replication::load_document_head_blocks(&self.db, &doc_id.to_string())
            .await
            .unwrap()
    }

    /// Merge the composite `cid` from `from`, with the field blocks it links.
    async fn receive(&self, from: &Node, doc_id: &DocID, (cid, bytes): &(Cid, Bytes)) {
        for link in Block::from_dag_cbor(bytes).unwrap().links.iter().flatten() {
            let data = from.blocks.get(&link.link).await.unwrap().unwrap();
            self.blocks.put(&link.link, &data).await.unwrap();
        }
        self.blocks.put(cid, bytes).await.unwrap();
        let outcome = self
            .handler
            .handle_block(
                cid,
                bytes,
                BlockMetadata::normal(
                    &doc_id.to_string(),
                    COLLECTION_ID,
                    "did:key:z6MkrParkedEncrypted",
                    Some("peer"),
                    false,
                ),
            )
            .await
            .unwrap();
        assert!(outcome.is_merged(), "got {outcome:?}");
    }

    /// The encryption CID of the body block a composite links.
    async fn body_key(&self, (_, bytes): &(Cid, Bytes)) -> Cid {
        for link in Block::from_dag_cbor(bytes).unwrap().links.iter().flatten() {
            let data = self.blocks.get(&link.link).await.unwrap().unwrap();
            let block = Block::from_dag_cbor(&data).unwrap();
            if block
                .delta
                .field()
                .is_some_and(|field| field.field_name == "body")
            {
                return block.encryption.expect("the body is encrypted");
            }
        }
        panic!("no body block")
    }

    async fn body(&self, doc_id: &DocID) -> NormalValue {
        LensedAutoCommitFetcher::new(self.db.clone())
            .get_all("Notes")
            .await
            .unwrap()
            .into_iter()
            .find(|doc| doc.id() == Some(doc_id))
            .expect("the document is on the node")
            .get("body")
            .cloned()
            .unwrap_or(NormalValue::Null)
    }
}

fn text(value: &str) -> NormalValue {
    NormalValue::String(value.into())
}

/// One document's body: created on `a`, then edited on `a` and `b` at once,
/// every block reaching `c`, which parks the body's. Returns the document,
/// the key of its first body block, and the body both `a` and `b` reach.
async fn history(a: &Node, b: &Node, c: &Node, body: &str) -> (DocID, Cid, NormalValue) {
    let doc_id = a.create(body).await;
    let mut first = None;
    for head in a.heads(&doc_id).await {
        first.get_or_insert(a.body_key(&head).await);
        b.receive(a, &doc_id, &head).await;
        c.receive(a, &doc_id, &head).await;
    }
    let mut whole = a.document(Some(&doc_id));
    whole.set("body", text(&format!("<{body}"))).unwrap();
    let from_a = a.update(whole).await;
    let mut ops = b.document(Some(&doc_id));
    let end = body.chars().count() as u64;
    ops.edit(
        "body",
        FieldOp::Edit(vec![TextOp::Insert {
            pos: end,
            text: ">".into(),
        }]),
    )
    .unwrap();
    let from_b = b.update(ops).await;
    a.receive(b, &doc_id, &from_b).await;
    b.receive(a, &doc_id, &from_a).await;
    c.receive(a, &doc_id, &from_a).await;
    c.receive(b, &doc_id, &from_b).await;
    let merged = a.body(&doc_id).await;
    assert_eq!(b.body(&doc_id).await, merged);
    assert_eq!(merged, text(&format!("<{body}>")));
    assert_eq!(c.body(&doc_id).await, NormalValue::Null, "c parks the body");
    (doc_id, first.unwrap(), merged)
}

fn encrypt_body() {
    set_encryption_config(Some(EncryptionConfig {
        encrypt_doc: false,
        encrypt_fields: vec!["body".to_owned()],
    }));
}

fn every_key(vault: &Vault) -> RapidHashSet<Cid> {
    let mut held = RapidHashSet::new();
    vault.keys.iter().for_each(|(cid, _)| {
        held.insert(cid);
    });
    held
}

/// Every key local: the three parked blocks merge, the last of them
/// concurrent with the one before, so its merge decrypts the blocks below it.
#[test]
fn parked_blocks_of_an_encrypted_document_replay_from_local_keys() {
    workers::block_on(async {
        encrypt_body();
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::default());
        let a = Node::in_memory(&vault).await;
        let b = Node::in_memory(&vault).await;
        let c = Node::on_disk(dir.path(), false, Kms::new(&vault, None), true).await;
        let (doc_id, _, merged) = history(&a, &b, &c, "ab").await;
        c.close().await;

        let c = Node::on_disk(
            dir.path(),
            true,
            Kms::new(&vault, Some(every_key(&vault))),
            false,
        )
        .await;
        assert_eq!(c.handler.replay_parked().await.unwrap(), 3);
        assert_eq!(c.body(&doc_id).await, merged);
        assert_eq!(c.kms.asked.load(Ordering::SeqCst), 0, "no peer is asked");
        c.close().await;
        set_encryption_config(None);
    })
    .unwrap()
}

/// The key of one document's first block is held elsewhere: that block and
/// the two whose history it is in stay parked, the other document's merge,
/// no peer is asked, and once the key is local the three merge too.
#[test]
fn a_parked_block_whose_history_needs_a_key_held_elsewhere_stays_parked() {
    workers::block_on(async {
        encrypt_body();
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::default());
        let a = Node::in_memory(&vault).await;
        let b = Node::in_memory(&vault).await;
        let c = Node::on_disk(dir.path(), false, Kms::new(&vault, None), true).await;
        let (kept, missing, kept_body) = history(&a, &b, &c, "ab").await;
        let (other, _, other_body) = history(&a, &b, &c, "cd").await;
        c.close().await;

        let mut held = every_key(&vault);
        held.remove(&missing);
        let c = Node::on_disk(dir.path(), true, Kms::new(&vault, Some(held)), false).await;
        assert_eq!(c.handler.replay_parked().await.unwrap(), 3);
        assert_eq!(c.body(&other).await, other_body);
        assert_eq!(c.body(&kept).await, NormalValue::Null);
        assert_eq!(c.kms.asked.load(Ordering::SeqCst), 0, "no peer is asked");
        c.close().await;

        let c = Node::on_disk(
            dir.path(),
            true,
            Kms::new(&vault, Some(every_key(&vault))),
            false,
        )
        .await;
        assert_eq!(
            c.handler.replay_parked().await.unwrap(),
            3,
            "they stayed parked"
        );
        assert_eq!(c.body(&kept).await, kept_body);
        assert_eq!(c.kms.asked.load(Ordering::SeqCst), 0);
        c.close().await;
        set_encryption_config(None);
    })
    .unwrap()
}
