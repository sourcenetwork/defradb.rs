//! App merge governance: a validator's verdicts, re-drive of deferred
//! composites when what they await merges, and recovery.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use blockstore::Blockstore as _;
use blockstore::DefraBlockstore;
use cid::Cid;
use crypto::PrivateKey as _;
use db::database::{DbOptions, DB};
use db::merge::governance::{
    FieldValue, MergeCandidate, MergeGovernance, MergeValidator, MergeVerdict, MergeView,
    SignatureStatus,
};
use db::merge::merge_handler::DbMergeHandler;
use db::AutoCommitFetcher;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload, Signature, SignatureHeader,
    SignatureType,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::NormalValue;
use query::runner::DocFetcher;
use schema::{CollectionVersion, FieldDescription, FieldKind};
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

/// A note is accepted when the grant it names is held and names the note's
/// verified signer as writer; its absence defers on the grant's CID.
#[derive(Default)]
struct GrantValidator {
    seen: Mutex<Vec<(Cid, SignatureStatus)>>,
}

impl GrantValidator {
    fn seen(&self, cid: &Cid) -> Vec<SignatureStatus> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(seen, _)| seen == cid)
            .map(|(_, status)| status.clone())
            .collect()
    }
}

#[async_trait]
impl MergeValidator for GrantValidator {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        self.seen
            .lock()
            .unwrap()
            .push((*candidate.cid, candidate.signature.clone()));
        let signer = match &candidate.signature {
            SignatureStatus::Verified(did) => did.clone(),
            SignatureStatus::NotHeld(signature) => {
                return Ok(MergeVerdict::defer("signature not held", [*signature]))
            }
            other => return Ok(MergeVerdict::reject(format!("bad signature: {other:?}"))),
        };
        let fields = view
            .composite_fields(candidate.cid)
            .await?
            .unwrap_or_default();
        let grant = fields.iter().find_map(|(name, value)| match value {
            FieldValue::Value(NormalValue::String(grant)) if name == "grant" => {
                Cid::try_from(grant.as_str()).ok()
            }
            _ => None,
        });
        let Some(grant) = grant else {
            return Ok(MergeVerdict::reject("note names no grant"));
        };
        let Some(grant_fields) = view.composite_fields(&grant).await? else {
            return Ok(MergeVerdict::defer("grant not held", [grant]));
        };
        let writer = grant_fields.iter().any(|(name, value)| {
            name == "writer" && *value == FieldValue::Value(NormalValue::String(signer.clone()))
        });
        Ok(if writer {
            MergeVerdict::Accept
        } else {
            MergeVerdict::reject("grant does not name the signer")
        })
    }
}

struct Node {
    db: Arc<DB<RegolithStore>>,
    blockstore: Arc<Blocks>,
    handler: DbMergeHandler<RegolithStore, Blocks>,
}

impl Node {
    async fn open(store: RegolithStore, governance: MergeGovernance, create: bool) -> Self {
        let store = Arc::new(store);
        let db = Arc::new(
            DB::open_from_arc_with_options(store.clone(), DbOptions::default())
                .await
                .unwrap(),
        );
        if create {
            for (name, id, field) in [
                ("Grants", "col-grants", "writer"),
                ("Notes", "col-notes", "grant"),
            ] {
                db.create_collection(CollectionVersion::new(
                    name,
                    id,
                    id,
                    vec![
                        FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                        FieldDescription::new("2", field, FieldKind::string()),
                    ],
                ))
                .await
                .unwrap();
            }
        }
        db.set_merge_governance(governance);
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
        Self {
            db,
            blockstore,
            handler,
        }
    }

    async fn doc_ids(&self, collection: &str) -> Vec<String> {
        AutoCommitFetcher::new(self.db.clone())
            .get_all(collection)
            .await
            .unwrap()
            .iter()
            .filter_map(|doc| doc.id().map(|id| id.to_string()))
            .collect()
    }
}

/// A genesis document held by its author, not yet sent to the node.
struct Genesis {
    cid: Cid,
    doc_id: String,
    blocks: Vec<(Cid, Vec<u8>)>,
    collection_id: &'static str,
}

fn genesis(collection_id: &'static str, field: &str, value: &str, by: &Signer) -> Genesis {
    authored(collection_id, None, field, value, by)
}

/// A composite setting `field`, as the genesis or as an update of `update_of`.
fn authored(
    collection_id: &'static str,
    update_of: Option<&Genesis>,
    field: &str,
    value: &str,
    by: &Signer,
) -> Genesis {
    let priority = if update_of.is_some() { 2 } else { 1 };
    let heads: Vec<Cid> = update_of.map(|genesis| genesis.cid).into_iter().collect();
    let mut blocks = Vec::new();
    let field_block = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: field.to_string(),
            priority,
            schema_version_id: collection_id.to_string(),
            data: db::block::builder::encode_value_as_cbor(&NormalValue::String(value.to_string()))
                .unwrap(),
        }),
        vec![],
        vec![],
    );
    let field_cid = field_block.generate_cid().unwrap();
    blocks.push((field_cid, field_block.to_dag_cbor().unwrap()));

    let mut composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: collection_id.to_string(),
            priority,
            status: 1,
        }),
        heads,
        vec![DAGLink::new(field, field_cid)],
    );
    let value = by.key.sign(&composite.to_dag_cbor().unwrap()).unwrap();
    let signature = Signature::new(
        SignatureHeader::new(
            SignatureType::EdDSA,
            hex::encode(by.key.public_key().raw()).into_bytes(),
        ),
        value,
    );
    let signature_cid = signature.generate_cid().unwrap();
    blocks.push((signature_cid, signature.to_dag_cbor().unwrap()));
    composite.signature = Some(signature_cid);
    let cid = composite.generate_cid().unwrap();
    blocks.push((cid, composite.to_dag_cbor().unwrap()));

    Genesis {
        cid,
        doc_id: db::block::builder::derive_doc_id(&update_of.map_or(cid, |genesis| genesis.cid)),
        blocks,
        collection_id,
    }
}

impl Genesis {
    async fn store(&self, node: &Node) {
        for (cid, bytes) in &self.blocks {
            node.blockstore.put(cid, bytes).await.unwrap();
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.blocks.last().unwrap().1
    }

    async fn merge(&self, node: &Node, creator: &str) -> MergeOutcome {
        self.store(node).await;
        node.handler
            .handle_block(
                &self.cid,
                self.bytes(),
                BlockMetadata::normal(
                    &self.doc_id,
                    self.collection_id,
                    creator,
                    Some("peer"),
                    false,
                ),
            )
            .await
            .unwrap()
    }
}

fn governance(validator: &Arc<GrantValidator>) -> MergeGovernance {
    MergeGovernance::new(["Notes"]).with_validator(validator.clone())
}

#[tokio::test]
async fn validator_accepts_rejects_and_defers() {
    let validator = Arc::new(GrantValidator::default());
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        governance(&validator),
        true,
    )
    .await;
    let writer = signer();
    let stranger = signer();

    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    grant.store(&node).await;

    let accepted = genesis("col-notes", "grant", &grant.cid.to_string(), &writer);
    assert_eq!(
        accepted.merge(&node, &writer.did).await,
        MergeOutcome::Merged
    );

    let forged = genesis("col-notes", "grant", &grant.cid.to_string(), &stranger);
    assert_eq!(
        forged.merge(&node, &writer.did).await,
        MergeOutcome::rejected("grant does not name the signer")
    );

    let unheld = genesis("col-grants", "writer", &stranger.did, &stranger);
    let waiting = genesis("col-notes", "grant", &unheld.cid.to_string(), &stranger);
    assert_eq!(
        waiting.merge(&node, &stranger.did).await,
        MergeOutcome::retryable_skip("grant not held")
    );

    assert_eq!(node.doc_ids("Notes").await, vec![accepted.doc_id.clone()]);
}

#[tokio::test]
async fn deferred_composite_merges_when_its_dependency_merges() {
    let validator = Arc::new(GrantValidator::default());
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        governance(&validator),
        true,
    )
    .await;
    let writer = signer();

    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let note = genesis("col-notes", "grant", &grant.cid.to_string(), &writer);
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("grant not held")
    );
    assert_eq!(node.handler.deferred_composites(), 1);
    assert!(node.doc_ids("Notes").await.is_empty());

    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);

    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);
    assert_eq!(node.handler.deferred_composites(), 0);
    assert_eq!(
        validator.seen(&note.cid),
        vec![
            SignatureStatus::Verified(writer.did.clone()),
            SignatureStatus::Verified(writer.did.clone()),
        ]
    );
}

#[tokio::test]
async fn redrive_verifies_the_signature_at_dispatch() {
    let validator = Arc::new(GrantValidator::default());
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        governance(&validator),
        true,
    )
    .await;
    let writer = signer();

    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let mut note = genesis("col-notes", "grant", &grant.cid.to_string(), &writer);
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("grant not held")
    );

    let signature_cid = note.blocks[1].0;
    let impostor = signer();
    let forged = Signature::new(
        SignatureHeader::new(
            SignatureType::EdDSA,
            hex::encode(impostor.key.public_key().raw()).into_bytes(),
        ),
        impostor.key.sign(b"not this block").unwrap(),
    );
    note.blocks[1].1 = forged.to_dag_cbor().unwrap();
    node.blockstore.delete(&signature_cid).await.unwrap();
    node.blockstore
        .put(&signature_cid, &note.blocks[1].1)
        .await
        .unwrap();

    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);

    assert!(node.doc_ids("Notes").await.is_empty());
    assert_eq!(validator.seen(&note.cid).len(), 1);
}

#[tokio::test]
async fn claimed_collection_without_validator_defers() {
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]),
        true,
    )
    .await;
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);

    assert!(matches!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    ));
    assert!(node.doc_ids("Notes").await.is_empty());
}

#[tokio::test]
async fn deferred_forgery_stays_unmerged_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node");
    let writer = signer();
    let forger = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let forged = genesis("col-notes", "grant", &grant.cid.to_string(), &forger);

    {
        let validator = Arc::new(GrantValidator::default());
        let node = Node::open(
            RegolithStore::open(&path).unwrap(),
            governance(&validator),
            true,
        )
        .await;
        assert_eq!(
            forged.merge(&node, &writer.did).await,
            MergeOutcome::retryable_skip("grant not held")
        );
        grant.store(&node).await;
        node.db.close().await.unwrap();
    }

    let validator = Arc::new(GrantValidator::default());
    let node = Node::open(
        RegolithStore::open(&path).unwrap(),
        governance(&validator),
        false,
    )
    .await;
    let outcome = node
        .handler
        .handle_block(
            &forged.cid,
            forged.bytes(),
            BlockMetadata::recovered(
                &forged.doc_id,
                forged.collection_id,
                &writer.did,
                Some(writer.did.clone()),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::rejected("grant does not name the signer")
    );
    assert_eq!(
        validator.seen(&forged.cid),
        vec![SignatureStatus::Verified(forger.did.clone())]
    );
    assert!(node.doc_ids("Notes").await.is_empty());
}

/// Accepts a signed genesis, and an update once its document's genesis is
/// held; defers an update on the genesis it names otherwise.
#[derive(Default)]
struct GenesisFirst {
    seen: Mutex<Vec<(Cid, SignatureStatus)>>,
}

#[async_trait]
impl MergeValidator for GenesisFirst {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        self.seen
            .lock()
            .unwrap()
            .push((*candidate.cid, candidate.signature.clone()));
        match &candidate.signature {
            SignatureStatus::Verified(_) => {}
            SignatureStatus::NotHeld(signature) => {
                return Ok(MergeVerdict::defer("signature not held", [*signature]))
            }
            other => return Ok(MergeVerdict::reject(format!("bad signature: {other:?}"))),
        }
        if candidate.is_genesis {
            return Ok(MergeVerdict::Accept);
        }
        let parent = candidate.block.heads.iter().flatten().next().copied();
        match (view.genesis(candidate.cid).await?, parent) {
            (Some(_), _) => Ok(MergeVerdict::Accept),
            (None, Some(parent)) => Ok(MergeVerdict::defer("genesis not held", [parent])),
            (None, None) => Ok(MergeVerdict::reject("update names no parent")),
        }
    }
}

async fn field_value(node: &Node, collection: &str, doc_id: &str, field: &str) -> Option<String> {
    AutoCommitFetcher::new(node.db.clone())
        .get_all(collection)
        .await
        .unwrap()
        .into_iter()
        .find(|doc| doc.id().is_some_and(|id| id.to_string() == doc_id))
        .and_then(|doc| match doc.get(field) {
            Some(NormalValue::String(value)) => Some(value.clone()),
            _ => None,
        })
}

fn merge_block(authored: &Genesis, creator: &str) -> defra_core::merge::MergeBlock {
    defra_core::merge::MergeBlock {
        cid: authored.cid,
        block_data: bytes::Bytes::copy_from_slice(authored.bytes()),
        doc_id: authored.doc_id.clone(),
        collection_id: authored.collection_id.to_string(),
        creator: creator.to_string(),
        sender_peer: Some("peer".to_string()),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    }
}

#[tokio::test]
async fn update_awaiting_its_genesis_merges_when_the_genesis_merges() {
    let validator = Arc::new(GenesisFirst::default());
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(validator.clone()),
        true,
    )
    .await;
    let writer = signer();
    let created = genesis("col-notes", "grant", "first", &writer);
    let updated = authored("col-notes", Some(&created), "grant", "second", &writer);

    assert_eq!(
        updated.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("document genesis not held")
    );
    assert_eq!(node.handler.deferred_composites(), 1);

    assert_eq!(
        created.merge(&node, &writer.did).await,
        MergeOutcome::Merged
    );

    assert_eq!(
        field_value(&node, "Notes", &created.doc_id, "grant").await,
        Some("second".to_string())
    );
    assert_eq!(node.handler.deferred_composites(), 0);
}

#[tokio::test]
async fn batch_deferral_is_released_by_a_batch_merge() {
    let validator = Arc::new(GenesisFirst::default());
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(validator.clone()),
        true,
    )
    .await;
    let writer = signer();
    let created = genesis("col-notes", "grant", "first", &writer);
    let updated = authored("col-notes", Some(&created), "grant", "second", &writer);

    updated.store(&node).await;
    let deferred = node
        .handler
        .handle_block_batch(&[merge_block(&updated, &writer.did)])
        .await;
    assert_eq!(
        deferred.into_iter().next().unwrap().unwrap(),
        MergeOutcome::retryable_skip("document genesis not held")
    );
    assert_eq!(node.handler.deferred_composites(), 1);

    created.store(&node).await;
    let merged = node
        .handler
        .handle_block_batch(&[merge_block(&created, &writer.did)])
        .await;
    assert_eq!(
        merged.into_iter().next().unwrap().unwrap(),
        MergeOutcome::Merged
    );

    assert_eq!(
        field_value(&node, "Notes", &created.doc_id, "grant").await,
        Some("second".to_string())
    );
    assert_eq!(node.handler.deferred_composites(), 0);
}

#[tokio::test]
async fn awaiting_a_non_composite_cid_waits_for_the_retry_clock() {
    let validator = Arc::new(GenesisFirst::default());
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(validator.clone()),
        true,
    )
    .await;
    let writer = signer();
    let created = genesis("col-notes", "grant", "first", &writer);
    let updated = authored("col-notes", Some(&created), "grant", "second", &writer);
    let (signature_cid, signature) = created.blocks[1].clone();
    for (cid, bytes) in created
        .blocks
        .iter()
        .filter(|(cid, _)| *cid != signature_cid)
    {
        node.blockstore.put(cid, bytes).await.unwrap();
    }

    assert_eq!(
        updated.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("signature not held")
    );
    node.blockstore
        .put(&signature_cid, &signature)
        .await
        .unwrap();
    let unrelated = genesis("col-notes", "grant", "unrelated", &writer);
    assert_eq!(
        unrelated.merge(&node, &writer.did).await,
        MergeOutcome::Merged
    );
    assert_eq!(
        field_value(&node, "Notes", &created.doc_id, "grant").await,
        None
    );
    assert_eq!(node.handler.deferred_composites(), 1);

    assert_eq!(
        updated.merge(&node, &writer.did).await,
        MergeOutcome::Merged
    );
    assert_eq!(
        field_value(&node, "Notes", &created.doc_id, "grant").await,
        Some("second".to_string())
    );
}

#[tokio::test]
async fn governed_collection_is_not_resolved_from_the_carrier() {
    let validator = Arc::new(GrantValidator::default());
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        governance(&validator),
        true,
    )
    .await;
    let writer = signer();
    let mut stray = genesis("unknown-version", "grant", "anything", &writer);
    stray.collection_id = "col-notes";

    assert_eq!(
        stray.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip(
            "schema version unknown-version is not held; a governed collection is resolved only from the block"
        )
    );
    assert!(validator.seen(&stray.cid).is_empty());
    assert!(node.doc_ids("Notes").await.is_empty());
}

/// Accepts when `field` of `collection` has a merged document equal to "held".
struct LookupValidator {
    collection: &'static str,
    field: &'static str,
}

#[async_trait]
impl MergeValidator for LookupValidator {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let matches = view
            .find_documents(
                self.collection,
                self.field,
                &NormalValue::String("held".to_string()),
            )
            .await?;
        Ok(if matches.is_empty() {
            MergeVerdict::defer("nothing matches", [])
        } else {
            MergeVerdict::Accept
        })
    }
}

#[tokio::test]
async fn find_documents_refuses_a_mutable_field() {
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(Arc::new(LookupValidator {
            collection: "Grants",
            field: "writer",
        })),
        true,
    )
    .await;
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    note.store(&node).await;

    let error = node
        .handler
        .handle_block(
            &note.cid,
            note.bytes(),
            BlockMetadata::normal(&note.doc_id, note.collection_id, &writer.did, None, false),
        )
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("must be an @immutable scalar LWW field"),
        "{error}"
    );
    assert!(node.doc_ids("Notes").await.is_empty());
}

#[tokio::test]
async fn find_documents_matches_an_immutable_field() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(
        DB::open_from_arc_with_options(store.clone(), DbOptions::default())
            .await
            .unwrap(),
    );
    let mut writer_field = FieldDescription::new("2", "writer", FieldKind::string());
    writer_field.immutable = true;
    db.create_collection(CollectionVersion::new(
        "Grants",
        "col-grants",
        "col-grants",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            writer_field,
        ],
    ))
    .await
    .unwrap();
    db.create_collection(CollectionVersion::new(
        "Notes",
        "col-notes",
        "col-notes",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "grant", FieldKind::string()),
        ],
    ))
    .await
    .unwrap();
    db.set_merge_governance(MergeGovernance::new(["Notes"]).with_validator(Arc::new(
        LookupValidator {
            collection: "Grants",
            field: "writer",
        },
    )));
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let node = Node {
        handler: DbMergeHandler::new(db.clone(), blockstore.clone()),
        db,
        blockstore,
    };
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);

    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("nothing matches")
    );
    let grant = genesis("col-grants", "writer", "held", &writer);
    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);
}
