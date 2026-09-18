//! A collection definition learned over p2p is rebuilt from its delta and
//! added to the runtime collection cache. That cache is keyed by name, and the
//! insert is unconditional, so a definition that merely shares a name with a
//! collection this node already has takes its place — dropping everything the
//! delta cannot carry.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CollectionDefinitionDeltaPayload, CompositeDeltaPayload, CrdtDelta, DAGLink,
    FieldDefinitionDeltaPayload, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::NormalValue;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind, PolicyDescription};
use storage::RegolithStore;

/// Go's `IntToFieldKind` numeric kind for a nillable string.
const STRING_KIND: u8 = 11;

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

fn policy() -> PolicyDescription {
    PolicyDescription {
        id: "policy-agents".to_string(),
        resource_name: "agents".to_string(),
    }
}

/// A node holding `Agent` with a policy and an `@immutable` field.
async fn node() -> (
    Arc<DB<RegolithStore>>,
    Handler,
    Arc<DefraBlockstore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let mut did = FieldDescription::new("2", "did", FieldKind::string());
    did.immutable = true;
    db.create_collection(
        CollectionVersion::new(
            "Agent",
            "col-agent",
            "col-agent",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                did,
            ],
        )
        .with_policy(policy()),
    )
    .await
    .unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    (db, handler, blockstore)
}

/// A definition block naming `collection`, as a peer sends it.
fn definition(collection: &str, fields: &[&str]) -> (Cid, Vec<(Cid, Vec<u8>)>) {
    let mut blocks = Vec::new();
    let mut links = Vec::new();
    for field in fields {
        let block = Block::new(
            CrdtDelta::FieldDefinition(
                FieldDefinitionDeltaPayload::new(1)
                    .with_name(*field)
                    .with_scalar_kind(STRING_KIND)
                    .with_crdt(CType::LwwRegister.to_u8()),
            ),
            vec![],
            vec![],
        );
        let cid = block.generate_cid().unwrap();
        blocks.push((cid, block.to_dag_cbor().unwrap()));
        links.push(DAGLink::new(*field, cid));
    }
    let block = Block::new(
        CrdtDelta::CollectionDefinition(
            CollectionDefinitionDeltaPayload::new(1).with_name(collection),
        ),
        vec![],
        links,
    );
    let cid = block.generate_cid().unwrap();
    blocks.push((cid, block.to_dag_cbor().unwrap()));
    (cid, blocks)
}

async fn merge(
    handler: &Handler,
    blockstore: &DefraBlockstore<RegolithStore>,
    collection: &str,
) -> MergeOutcome {
    let (cid, blocks) = definition(collection, &["_docID", "did"]);
    for (block_cid, bytes) in &blocks {
        blockstore.put(block_cid, bytes).await.unwrap();
    }
    let bytes = &blocks.last().unwrap().1;
    handler
        .handle_block(
            &cid,
            bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap()
}

/// What the collection named `Agent` commits to, as this node reads it.
fn commitments(db: &DB<RegolithStore>) -> (Option<PolicyDescription>, bool, String) {
    let collection = db.get_collection("Agent").unwrap().unwrap();
    let schema = collection.schema();
    let immutable = schema
        .fields
        .iter()
        .any(|field| field.name == "did" && field.immutable);
    (
        schema.policy.clone(),
        immutable,
        schema.collection_id.clone(),
    )
}

#[tokio::test]
async fn a_synced_definition_does_not_displace_a_collection_of_the_same_name() {
    let (db, handler, blockstore) = node().await;
    assert_eq!(
        commitments(&db),
        (Some(policy()), true, "col-agent".to_string()),
        "the local definition holds its policy and its immutable field"
    );

    // The version is still stored and still reported merged; only the cache
    // entry, which can hold one collection per name, is left alone.
    assert_eq!(
        merge(&handler, &blockstore, "Agent").await,
        MergeOutcome::Merged
    );

    assert_eq!(
        commitments(&db),
        (Some(policy()), true, "col-agent".to_string()),
        "a definition delta carries no policy and no @immutable flag, so a \
         rebuilt record must not take the local one's place"
    );
}

/// The insert exists so a synced collection is visible to `get_collections`
/// with inactive included. A name this node does not have must still register.
#[tokio::test]
async fn a_synced_definition_for_a_free_name_still_registers() {
    let (db, handler, blockstore) = node().await;

    let outcome = merge(&handler, &blockstore, "Ledger").await;

    assert_eq!(outcome, MergeOutcome::Merged);
    let collection = db.get_collection("Ledger").unwrap().unwrap();
    assert_eq!(collection.schema().name, "Ledger");
    assert!(collection
        .schema()
        .fields
        .iter()
        .any(|field| field.name == "did"));
}

/// A genesis composite for `collection_version`, with its field block.
fn document(collection_version: &str, value: &str) -> (Cid, Vec<(Cid, Vec<u8>)>) {
    let field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "did".to_string(),
            priority: 1,
            schema_version_id: collection_version.to_string(),
            data: db::block::builder::encode_value_as_cbor(&NormalValue::String(value.to_string()))
                .unwrap(),
        }),
        vec![],
        vec![],
    );
    let field_cid = field.generate_cid().unwrap();
    let composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: collection_version.to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new("did", field_cid)],
    );
    let composite_cid = composite.generate_cid().unwrap();
    (
        composite_cid,
        vec![
            (field_cid, field.to_dag_cbor().unwrap()),
            (composite_cid, composite.to_dag_cbor().unwrap()),
        ],
    )
}

/// Keeping a peer's same-named collection out of the name-keyed cache must not
/// make its documents unmergeable: the definition is durably stored under its
/// own version, and that is what a composite naming it resolves through.
#[tokio::test]
async fn a_document_for_a_collection_kept_out_of_the_cache_still_merges() {
    let (_db, handler, blockstore) = node().await;
    let (definition_cid, definition_blocks) = definition("Agent", &["_docID", "did"]);
    for (cid, bytes) in &definition_blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }
    let definition_bytes = &definition_blocks.last().unwrap().1;
    handler
        .handle_block(
            &definition_cid,
            definition_bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();

    // The peer's collection is keyed by its own version id, which is the CID of
    // the definition block it sent.
    let peer_version = definition_cid.to_string();
    let (composite_cid, blocks) = document(&peer_version, "did:key:peer");
    for (cid, bytes) in &blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }
    let composite_bytes = &blocks.last().unwrap().1;

    let outcome = handler
        .handle_block(
            &composite_cid,
            composite_bytes,
            BlockMetadata::normal("", &peer_version, "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::Merged,
        "the collection is durably stored, so its documents must resolve it"
    );
}

/// A patched definition resolves its collection ID from the version it
/// supersedes, so it is the *same* collection and the cache takes it under the
/// live name. The delta expresses a name and its fields and nothing else, so
/// everything the superseded version held has to survive the rebuild — not
/// just the attributes someone remembered to list.
#[tokio::test]
async fn a_patched_definition_keeps_what_the_delta_cannot_carry() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let defined = query::parse_sdl(
        r#"type Agent @policy(id: "p1", resource: "agents") @branchable {
            did: String @immutable @index
        }"#,
    )
    .unwrap()
    .remove(0);
    db.create_collection(defined).await.unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());

    let held = db.get_collection("Agent").unwrap().unwrap();
    assert!(held.schema().policy.is_some() && held.schema().is_branchable);
    assert!(!held.schema().indexes.is_empty());
    let previous: Cid = held.schema().version_id.parse().expect("a CID version id");

    // A patch adding one field: no name, and the superseded version as its head.
    let field = Block::new(
        CrdtDelta::FieldDefinition(
            FieldDefinitionDeltaPayload::new(1)
                .with_name("body")
                .with_scalar_kind(STRING_KIND)
                .with_crdt(CType::LwwRegister.to_u8()),
        ),
        vec![],
        vec![],
    );
    let field_cid = field.generate_cid().unwrap();
    blockstore
        .put(&field_cid, &field.to_dag_cbor().unwrap())
        .await
        .unwrap();
    let patch = Block::new(
        CrdtDelta::CollectionDefinition(CollectionDefinitionDeltaPayload::new(2)),
        vec![previous],
        vec![DAGLink::new("body", field_cid)],
    );
    let patch_cid = patch.generate_cid().unwrap();
    let patch_bytes = patch.to_dag_cbor().unwrap();
    blockstore.put(&patch_cid, &patch_bytes).await.unwrap();

    handler
        .handle_block(
            &patch_cid,
            &patch_bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();

    let patched = db.get_collection("Agent").unwrap().unwrap();
    let patched = patched.schema();
    assert_eq!(
        patched.policy.as_ref().map(|policy| policy.id.as_str()),
        Some("p1"),
        "a patch carries no policy, so it must inherit the one it supersedes"
    );
    assert!(
        patched.is_branchable,
        "a patch carries no branchable flag either"
    );
    assert!(
        !patched.indexes.is_empty(),
        "a patch carries no indexes either, and this is the very entry the \
         merge path prefers for its index action state"
    );
    // The fields of a patched version start as the superseded version's, so an
    // existing field keeps its flags without anything rescuing them.
    assert!(
        patched
            .fields
            .iter()
            .any(|field| field.name == "did" && field.immutable),
        "the superseded version's fields carry their own flags"
    );
    assert!(patched.fields.iter().any(|field| field.name == "body"));
}

/// A view is a collection whose rows are computed from a query. A patch that
/// carries no `query_select` must not quietly turn one back into an ordinary
/// materialized collection.
#[tokio::test]
async fn a_patched_definition_does_not_unmake_a_view() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let mut view = CollectionVersion::new(
        "Tally",
        "col-tally",
        "col-tally",
        vec![FieldDescription::new("1", "_docID", FieldKind::doc_id())],
    );
    view.query = Some(schema::QuerySource::new(serde_json::json!({})));
    view.is_materialized = false;
    db.create_collection(view).await.unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());

    let held = db.get_collection("Tally").unwrap().unwrap();
    assert!(!held.schema().is_materialized && held.schema().query.is_some());
    let previous: Cid = held.schema().version_id.parse().expect("a CID version id");

    let patch = Block::new(
        CrdtDelta::CollectionDefinition(CollectionDefinitionDeltaPayload::new(2)),
        vec![previous],
        vec![],
    );
    let patch_cid = patch.generate_cid().unwrap();
    let patch_bytes = patch.to_dag_cbor().unwrap();
    blockstore.put(&patch_cid, &patch_bytes).await.unwrap();
    handler
        .handle_block(
            &patch_cid,
            &patch_bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();

    let patched = db.get_collection("Tally").unwrap().unwrap();
    assert!(
        patched.schema().query.is_some(),
        "the view's query must survive a patch that does not carry one"
    );
    assert!(
        !patched.schema().is_materialized,
        "and it must still be a view"
    );
}
