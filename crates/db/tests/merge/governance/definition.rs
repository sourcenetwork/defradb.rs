//! A definition learned over p2p is rebuilt from the delta alone, which
//! carries none of what a governed collection commits to.

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

/// A definition naming `collection` with one string `field`, as the p2p path
/// rebuilds it: a name, a field kind and a CRDT type, and nothing else.
fn definition(collection: &str, fields: &[&str]) -> Definition {
    let mut blocks = Vec::new();
    let mut links = Vec::new();
    for field in fields {
        let block = Block::new(
            CrdtDelta::FieldDefinition(
                FieldDefinitionDeltaPayload::new(1)
                    .with_name(*field)
                    .with_scalar_kind(STRING_KIND)
                    .with_crdt(schema::CType::LwwRegister.to_u8()),
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
