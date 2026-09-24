//! A definition learned over p2p is rebuilt from the delta alone. It carries
//! what a governed collection's identity commits to and nothing else, so the
//! rebuild restores that much and must never displace a local record holding
//! more.

use defra_core::block::{CollectionDefinitionDeltaPayload, FieldDefinitionDeltaPayload};
use schema::PolicyDescription;

use super::*;

/// Go's `IntToFieldKind` numeric kind for a nillable string.
const STRING_KIND: u8 = 11;

/// A collection definition as a peer holds it before sending: the definition
/// block and the field blocks it links.
struct Definition {
    cid: Cid,
    blocks: Vec<(Cid, Vec<u8>)>,
}

/// A definition naming `collection`, as a peer sends it. A field prefixed with
/// `!` is `@immutable`.
fn definition(collection: &str, fields: &[&str]) -> Definition {
    definition_with(collection, fields, None, false)
}

fn definition_with(
    collection: &str,
    fields: &[&str],
    governance_root: Option<&str>,
    is_branchable: bool,
) -> Definition {
    let mut blocks = Vec::new();
    let mut links = Vec::new();
    for field in fields {
        let immutable = field.starts_with('!');
        let name = field.trim_start_matches('!');
        let block = Block::new(
            CrdtDelta::FieldDefinition(
                FieldDefinitionDeltaPayload::new(1)
                    .with_name(name)
                    .with_scalar_kind(STRING_KIND)
                    .with_crdt(schema::CType::LwwRegister.to_u8())
                    .with_immutable(immutable),
            ),
            vec![],
            vec![],
        );
        let cid = block.generate_cid().unwrap();
        blocks.push((cid, block.to_dag_cbor().unwrap()));
        links.push(DAGLink::new(name, cid));
    }

    let mut payload = CollectionDefinitionDeltaPayload::new(1)
        .with_name(collection)
        .with_branchable(is_branchable);
    if let Some(root) = governance_root {
        payload = payload.with_governance_root(root);
    }
    let block = Block::new(CrdtDelta::CollectionDefinition(payload), vec![], links);
    let cid = block.generate_cid().unwrap();
    blocks.push((cid, block.to_dag_cbor().unwrap()));
    Definition { cid, blocks }
}

impl Definition {
    async fn merge(&self, node: &Node) -> MergeOutcome {
        for (cid, bytes) in &self.blocks {
            node.blockstore.put(cid, bytes).await.unwrap();
        }
        let bytes = &self.blocks.last().unwrap().1;
        node.handler
            .handle_block(
                &self.cid,
                bytes,
                BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
            )
            .await
            .unwrap()
    }
}

fn governed_policy() -> PolicyDescription {
    PolicyDescription {
        id: "policy-grants".to_string(),
        resource_name: "grants".to_string(),
    }
}

impl Node {
    /// A node holding nothing yet, governing `Grants`.
    async fn bare() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(
            DB::open_from_arc_with_options(store.clone(), DbOptions::default())
                .await
                .unwrap(),
        );
        db.set_merge_governance(
            MergeGovernance::new(["Grants"]).with_validator(Arc::new(AcceptEverything)),
        );
        Self::assemble(db, store)
    }

    /// Define `Grants` locally, with a policy and an `@immutable` `writer`.
    async fn define_grants(&self) {
        let mut writer = FieldDescription::new("2", "writer", FieldKind::string());
        writer.immutable = true;
        self.db
            .create_collection(
                CollectionVersion::new(
                    "Grants",
                    "col-grants",
                    "col-grants",
                    vec![
                        FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                        writer,
                    ],
                )
                .with_policy(governed_policy()),
            )
            .await
            .unwrap();
    }

    /// Define `Grants` locally, ungoverned: branchable, with an `@immutable`
    /// `writer` and no policy. An ungoverned identity commits to none of it,
    /// so no delta carries any of it either.
    async fn define_ungoverned_grants(&self) {
        let mut writer = FieldDescription::new("2", "writer", FieldKind::string());
        writer.immutable = true;
        let mut version = CollectionVersion::new(
            "Grants",
            "col-grants",
            "col-grants",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                writer,
            ],
        );
        version.is_branchable = true;
        self.db.create_collection(version).await.unwrap();
    }

    /// What the collection named `Grants` commits to, as this node reads it.
    fn grants_commitments(&self) -> (Option<PolicyDescription>, bool) {
        let collection = self.db.get_collection("Grants").unwrap().unwrap();
        let schema = collection.schema();
        let immutable = schema
            .fields
            .iter()
            .any(|field| field.name == "writer" && field.immutable);
        (schema.policy.clone(), immutable)
    }
}

struct AcceptEverything;

#[async_trait]
impl MergeValidator for AcceptEverything {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(MergeVerdict::Accept)
    }
}

#[tokio::test]
async fn a_synced_definition_does_not_strip_a_governed_collection() {
    let node = Node::bare().await;
    node.define_grants().await;
    assert_eq!(
        node.grants_commitments(),
        (Some(governed_policy()), true),
        "the local definition holds its policy and its immutable field"
    );

    definition("Grants", &["_docID", "writer"])
        .merge(&node)
        .await;

    assert_eq!(
        node.grants_commitments(),
        (Some(governed_policy()), true),
        "a synced definition carries neither, so it must not replace them"
    );
}

#[tokio::test]
async fn a_local_definition_replaces_one_synced_first() {
    let node = Node::bare().await;
    definition("Grants", &["_docID", "writer"])
        .merge(&node)
        .await;

    node.define_grants().await;

    assert_eq!(node.grants_commitments(), (Some(governed_policy()), true));
}

#[tokio::test]
async fn a_synced_definition_for_an_unknown_collection_still_registers() {
    let node = Node::bare().await;

    let outcome = definition("Ledgers", &["_docID", "writer"])
        .merge(&node)
        .await;

    assert_eq!(outcome, MergeOutcome::Merged);
    let collection = node.db.get_collection("Ledgers").unwrap().unwrap();
    assert_eq!(collection.schema().name, "Ledgers");
    assert!(collection
        .schema()
        .fields
        .iter()
        .any(|field| field.name == "writer"));
}

#[tokio::test]
async fn a_synced_definition_restores_the_commitments_it_carries() {
    let node = Node::bare().await;

    definition_with("Ledgers", &["_docID", "!writer"], Some("root-a"), true)
        .merge(&node)
        .await;

    let collection = node.db.get_collection("Ledgers").unwrap().unwrap();
    let schema = collection.schema();
    assert_eq!(schema.governance_root.as_deref(), Some("root-a"));
    assert!(schema.is_branchable);
    assert!(schema
        .fields
        .iter()
        .any(|field| field.name == "writer" && field.immutable));
}

#[tokio::test]
async fn a_synced_definition_under_another_root_does_not_displace_a_local_one() {
    let node = Node::bare().await;
    node.define_grants().await;

    definition_with("Grants", &["_docID", "!writer"], Some("root-b"), false)
        .merge(&node)
        .await;

    assert_eq!(node.grants_commitments(), (Some(governed_policy()), true));
    assert_eq!(
        node.db
            .get_collection("Grants")
            .unwrap()
            .unwrap()
            .schema()
            .governance_root,
        None,
        "the local record keeps its own root"
    );
}

/// A governed identity commits to the fields' immutability and to
/// branchability, so a definition under the same root without them names a
/// different collection, and the name-keyed cache must not let it take the
/// place of the record that holds them.
#[tokio::test]
async fn a_synced_definition_under_the_same_root_without_the_flags_does_not_displace() {
    let node = Node::bare().await;
    definition_with("Ledgers", &["_docID", "!writer"], Some("root-a"), true)
        .merge(&node)
        .await;
    let held = node.db.get_collection("Ledgers").unwrap().unwrap();
    let held_id = held.schema().collection_id.clone();

    definition_with("Ledgers", &["_docID", "writer"], Some("root-a"), false)
        .merge(&node)
        .await;

    let collection = node.db.get_collection("Ledgers").unwrap().unwrap();
    let schema = collection.schema();
    assert_eq!(
        schema.collection_id, held_id,
        "the weaker definition displaced the record"
    );
    assert!(schema.is_branchable, "branchable history was stripped");
    assert!(
        schema
            .fields
            .iter()
            .any(|field| field.name == "writer" && field.immutable),
        "@immutable was stripped"
    );
}

/// A definition block reproduces the identity it was built with on a node that
/// has never seen the collection: the identity is a function of the block, and
/// the block carries everything the identity commits to.
#[tokio::test]
async fn a_definition_block_reproduces_its_identity_on_a_fresh_node() {
    let author = Node::bare().await;
    let defined =
        query::parse_sdl(r#"type Ledgers @governed(root: "root-a") { writer: String @immutable }"#)
            .unwrap()
            .remove(0);
    author.db.create_collection(defined.clone()).await.unwrap();
    let authored = author.db.get_collection("Ledgers").unwrap().unwrap();
    let version_id = authored.schema().version_id.clone();
    let cid: Cid = version_id.parse().expect("the version ID names the block");
    let bytes = author.blockstore.get(&cid).await.unwrap().expect("block");

    let fresh = Node::bare().await;
    fresh.blockstore.put(&cid, &bytes).await.unwrap();
    for link in Block::from_dag_cbor(&bytes).unwrap().links.iter().flatten() {
        let field = author.blockstore.get(&link.link).await.unwrap().unwrap();
        fresh.blockstore.put(&link.link, &field).await.unwrap();
    }
    fresh
        .handler
        .handle_block(
            &cid,
            &bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();

    let synced = fresh.db.get_collection("Ledgers").unwrap().unwrap();
    assert_eq!(synced.schema().version_id, version_id);
    assert_eq!(synced.schema().collection_id, version_id);
    assert_eq!(synced.schema().governance_root.as_deref(), Some("root-a"));
    assert_eq!(synced.schema().collection_id, defined.collection_id);
    assert!(synced
        .schema()
        .fields
        .iter()
        .any(|field| field.name == "writer" && field.immutable));
}

/// The gate cuts both ways: an ungoverned collection's delta carries neither
/// its immutable flags nor its branchable flag, because its identity commits
/// to neither, so a synced definition of that name must still be kept out of
/// the cache even with no policy in sight.
#[tokio::test]
async fn a_synced_definition_does_not_strip_an_ungoverned_collection() {
    let node = Node::bare().await;
    node.define_ungoverned_grants().await;

    definition("Grants", &["_docID", "writer"])
        .merge(&node)
        .await;

    let collection = node.db.get_collection("Grants").unwrap().unwrap();
    let schema = collection.schema();
    assert!(schema.is_branchable, "branchable history was stripped");
    assert!(
        schema
            .fields
            .iter()
            .any(|field| field.name == "writer" && field.immutable),
        "the immutable flag was stripped"
    );
}

/// The same, for a governed collection carrying a policy. Its collection ID
/// and version ID differ — the policy reaches only the version — so the
/// receiver cannot read the collection ID off the block's own CID.
#[tokio::test]
async fn a_policied_definition_block_reproduces_its_identity_on_a_fresh_node() {
    let author = Node::bare().await;
    let defined = query::parse_sdl(
        r#"type Ledgers @governed(root: "root-a") @policy(id: "p1", resource: "ledgers") { writer: String @immutable }"#,
    )
    .unwrap()
    .remove(0);
    assert_ne!(
        defined.version_id, defined.collection_id,
        "a policy moves the version ID away from the collection ID"
    );
    author.db.create_collection(defined.clone()).await.unwrap();
    let authored = author.db.get_collection("Ledgers").unwrap().unwrap();
    let version_id = authored.schema().version_id.clone();
    let cid: Cid = version_id.parse().expect("the version ID names the block");
    let bytes = author.blockstore.get(&cid).await.unwrap().expect("block");

    let fresh = Node::bare().await;
    fresh.blockstore.put(&cid, &bytes).await.unwrap();
    for link in Block::from_dag_cbor(&bytes).unwrap().links.iter().flatten() {
        let field = author.blockstore.get(&link.link).await.unwrap().unwrap();
        fresh.blockstore.put(&link.link, &field).await.unwrap();
    }
    fresh
        .handler
        .handle_block(
            &cid,
            &bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();

    let synced = fresh.db.get_collection("Ledgers").unwrap().unwrap();
    assert_eq!(synced.schema().version_id, version_id);
    assert_eq!(
        synced.schema().collection_id,
        defined.collection_id,
        "the receiver must derive the same collection ID the author did"
    );
}

/// Carry the definition block named by `version_id`, with the field blocks it
/// links, from `from` to `to`, and merge it there as a peer's delivery.
async fn sync_definition(from: &Node, to: &Node, version_id: &str) -> MergeOutcome {
    let cid: Cid = version_id.parse().expect("the version ID names the block");
    let bytes = from.blockstore.get(&cid).await.unwrap().expect("block");
    to.blockstore.put(&cid, &bytes).await.unwrap();
    for link in Block::from_dag_cbor(&bytes).unwrap().links.iter().flatten() {
        let field = from.blockstore.get(&link.link).await.unwrap().unwrap();
        to.blockstore.put(&link.link, &field).await.unwrap();
    }
    to.handler
        .handle_block(
            &cid,
            &bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap()
}

const POLICIED_LEDGERS: &str = r#"type Ledgers @governed(root: "root-a") @policy(id: "p1", resource: "ledgers") { writer: String @immutable }"#;

/// The version ID binds the policy, but the delta carries only a CID over its
/// reference, so a node that has never held the policy rebuilds a record
/// without one. That record must not become the active version: activated,
/// it would serve the collection with no policy at all, indistinguishable
/// from one that never had one.
#[tokio::test]
async fn a_synced_version_bound_to_a_policy_this_node_does_not_hold_cannot_be_activated() {
    let author = Node::bare().await;
    let defined = query::parse_sdl(POLICIED_LEDGERS).unwrap().remove(0);
    author.db.create_collection(defined.clone()).await.unwrap();

    let fresh = Node::bare().await;
    sync_definition(&author, &fresh, &defined.version_id).await;

    let activated = fresh
        .db
        .set_active_collection_version(&defined.version_id)
        .await;
    assert!(
        activated.is_err(),
        "activated a version whose identity binds a policy the record does not hold"
    );
    let synced = fresh.db.get_collection("Ledgers").unwrap().unwrap();
    assert!(!synced.schema().is_active);
}

/// A node that holds the collection with its policy receives a patched
/// version from a peer. The patch binds the same policy, by CID, and the
/// node holds that reference, so the rebuilt version keeps it: activating
/// the patch must not turn the collection into an unpoliced one.
#[tokio::test]
async fn a_synced_patch_keeps_the_policy_this_node_holds() {
    let author = Node::bare().await;
    let defined = query::parse_sdl(POLICIED_LEDGERS).unwrap().remove(0);
    author.db.create_collection(defined.clone()).await.unwrap();
    let peer = Node::bare().await;
    peer.db.create_collection(defined.clone()).await.unwrap();

    let patched = author
        .db
        .patch_collection(
            "Ledgers",
            r#"[{"op": "add", "path": "/Ledgers/Fields/-", "value": {"Name": "note", "Kind": "String"}}]"#,
            None,
        )
        .await
        .unwrap();
    assert_ne!(patched.version_id, defined.version_id);
    sync_definition(&author, &peer, &patched.version_id).await;

    peer.db
        .set_active_collection_version(&patched.version_id)
        .await
        .unwrap();
    let active = peer.db.get_collection("Ledgers").unwrap().unwrap();
    assert_eq!(active.schema().version_id, patched.version_id);
    assert_eq!(
        active.schema().policy,
        defined.policy,
        "activating the synced patch dropped the policy"
    );
}

/// Splitting the collection ID from the version ID is a governed collection's
/// rule, and the root is what declares it. A `policy` link on a definition that
/// declares no root is not this tree's to write, so a receiver must read the
/// collection ID off the block's own CID rather than derive one its author
/// never derived and stop being that collection's replica.
#[tokio::test]
async fn an_ungoverned_policy_link_does_not_move_the_collection_id() {
    let node = Node::bare().await;
    let policy_cid = schema::generate_policy_cid(&PolicyDescription {
        id: "p1".to_string(),
        resource_name: "ledgers".to_string(),
    })
    .unwrap();
    let block = Block::new(
        CrdtDelta::CollectionDefinition(
            CollectionDefinitionDeltaPayload::new(1)
                .with_name("Ledgers")
                .with_policy_cid(policy_cid),
        ),
        vec![],
        vec![],
    );
    let cid = block.generate_cid().unwrap();
    let bytes = block.to_dag_cbor().unwrap();
    node.blockstore.put(&cid, &bytes).await.unwrap();

    assert_eq!(
        node.handler
            .handle_block(
                &cid,
                &bytes,
                BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
            )
            .await
            .unwrap(),
        MergeOutcome::Merged
    );

    let synced = node.db.get_collection("Ledgers").unwrap().unwrap();
    assert_eq!(
        synced.schema().collection_id,
        cid.to_string(),
        "an ungoverned definition names its collection by its own CID"
    );
    assert_eq!(synced.schema().version_id, cid.to_string());
}
