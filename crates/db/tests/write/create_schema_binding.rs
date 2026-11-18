use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

use defra_core::RequestContext;
use document::{Document, NormalValue, WritePreparation};
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use serde_json::{json, Value};

use super::*;

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn prepared_create_checks_immutable_definition_even_when_version_is_unchanged() {
    let initial = CollectionVersion::new(
        "Agent",
        "same-version",
        "agents",
        vec![
            FieldDescription::new("1", "agent_did", FieldKind::string(), CType::LwwRegister)
                .as_immutable(),
        ],
    );
    let mut doc = Document::new(initial.clone());
    doc.set("agent_did", NormalValue::String("did:key:agent".into()))
        .unwrap();
    let identity = DocRef::new(1, 1);
    let crdts = crdt::Crdts::default();
    let blocks = crate::block::builder::compute_document_blocks(
        &crdts, &doc, &initial, identity, None, None,
    )
    .unwrap();
    let prepared_cid = blocks.block_result.cid;
    let mut write = WritePreparation::new(identity.collection_short_id, identity.doc_short_id);
    write.set_payload(Arc::new(PreparedCreateBlocks {
        schema: Arc::new(initial.clone()),
        computed: blocks,
    }));
    doc.set_write_preparation(Arc::new(write));
    let mut changed = initial.clone();
    changed.fields[0].immutable = false;
    let definition = |schema: &CollectionVersion| {
        json!({
            "version": schema.version_id,
            "fields": schema.fields.iter().map(|field| json!({"name":field.name,"immutable":field.immutable})).collect::<Vec<_>>()
        })
    };
    let input = json!([{"prepared":definition(&initial),"current":definition(&initial)},
                       {"prepared":definition(&initial),"current":definition(&changed)}]);
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "ClientCreateSchema.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{input}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (index, schema) in [initial, changed].into_iter().enumerate() {
        let collection = Collection::new(schema, crdts.clone());
        let actual = compute(
            &collection,
            &doc,
            identity,
            &RequestContext::default(),
            None,
        )
        .unwrap();
        assert_eq!(
            actual.block_result.cid == prepared_cid,
            cases[index]["reusable"].as_bool().unwrap()
        );
    }
}
