//! Go scenarios that defradb.rs runs with a step changed, by scenario ID: the
//! edits and the reason each generated test carries. A deviation keeps what
//! the scenario checks and changes only a value Go's harness supplies that
//! cannot hold on defradb.rs. Generation applies the edits, and the round trip
//! applies them to the corpus scenario before it compares.

use serde_json::Value;

/// One change to a scenario's steps, in the corpus's wire form.
enum Edit {
    /// Every step of op `op` also captures its result at `pointer` as `name`.
    Capture {
        op: &'static str,
        name: &'static str,
        pointer: &'static str,
    },
    /// Every step of op `op` verifies the block captured as `name`.
    BlockRef {
        op: &'static str,
        name: &'static str,
    },
    /// Every verify step whose signer is client seed `seed` verifies against
    /// node `node`'s signing key instead.
    SignerNode { seed: u64, node: &'static str },
    /// Every `explainScan` matcher expects scan metric `metric` above zero
    /// rather than Go's exact count.
    PositiveScanMetric { metric: &'static str },
}

const BRANCHABLE: &[Edit] = &[
    Edit::Capture {
        op: "peer.awaitUpdates",
        name: "collectionHead",
        pointer: "/cids/1",
    },
    Edit::BlockRef {
        op: "signature.verifySeed",
        name: "collectionHead",
    },
];

const BRANCHABLE_REASON: &str =
    "Go verifies the literal CID of its collection-level block, which Go's \
     key signed under Go's policy ID, so no defradb.rs node can write it; the test verifies the \
     collection-level block the sync reports instead.";

const NODE_SIGNER: &[Edit] = &[Edit::SignerNode {
    seed: 0,
    node: "db",
}];

const NODE_SIGNER_REASON: &str = "Go's signer is NodeIdentity(0), the node's own key, which the \
     corpus wrote as client identity 0; the test verifies against node db's signing key.";

const VECTOR_INDEX_FETCHES: &[Edit] = &[Edit::PositiveScanMetric {
    metric: "indexFetches",
}];

const VECTOR_INDEX_FETCHES_REASON: &str = "Go reports a routed vector search as one index fetch; \
     defradb.rs counts the index entries the search reads, which depends on the graph, so the test \
     expects a positive count, which Go's 1 also meets.";

const ENTRIES: &[(&str, &[Edit], &str)] = &[
    (
        "query.simple.query-simple-with-similarity-on-vector-index-returns-k-nearest",
        VECTOR_INDEX_FETCHES,
        VECTOR_INDEX_FETCHES_REASON,
    ),
    (
        "signature.signature-acp-with-branchable-collection-if-has-collection-access-validate-signature",
        BRANCHABLE,
        BRANCHABLE_REASON,
    ),
    (
        "signature.signature-acp-with-branchable-collection-if-has-no-collection-access-should-error",
        BRANCHABLE,
        BRANCHABLE_REASON,
    ),
    (
        "acp.nac.nac-gates-verify-signature-authorized-identity-allow-access",
        NODE_SIGNER,
        NODE_SIGNER_REASON,
    ),
    (
        "acp.nac.relation_admin.nac-admin-relation-can-verify-signature",
        NODE_SIGNER,
        NODE_SIGNER_REASON,
    ),
];

/// Applies scenario `id`'s deviation to `steps`, the corpus steps in wire
/// form, and returns its reason; `None`, and `steps` untouched, when the
/// scenario has none.
pub fn apply(id: &str, steps: &mut [Value]) -> Option<&'static str> {
    let (_, edits, reason) = ENTRIES.iter().find(|(entry, _, _)| *entry == id)?;
    for edit in *edits {
        for step in steps.iter_mut() {
            edit_step(edit, step);
        }
    }
    Some(reason)
}

fn edit_step(edit: &Edit, step: &mut Value) {
    if let Edit::PositiveScanMetric { metric } = edit {
        positive_scan_metric(step, metric);
        return;
    }
    let Some(map) = step.as_object_mut() else {
        return;
    };
    let op = map.get("op").and_then(Value::as_str).unwrap_or_default();
    match edit {
        Edit::Capture {
            op: target,
            name,
            pointer,
        } if op == *target => {
            let capture = map
                .entry("capture")
                .or_insert_with(|| Value::Object(Default::default()));
            if let Some(capture) = capture.as_object_mut() {
                capture.insert((*name).to_owned(), Value::String((*pointer).to_owned()));
            }
        }
        Edit::BlockRef { op: target, name } if op == *target => {
            if let Some(args) = map.get_mut("args").and_then(Value::as_object_mut) {
                args.insert("blockCID".to_owned(), serde_json::json!({ "$ref": name }));
            }
        }
        Edit::SignerNode { seed, node } if op == "signature.verifySeed" => {
            let Some(args) = map.get_mut("args").and_then(Value::as_object_mut) else {
                return;
            };
            if args.get("signerSeed").and_then(Value::as_u64) != Some(*seed) {
                return;
            }
            args.shift_remove("signerSeed");
            args.insert("signerNode".to_owned(), Value::String((*node).to_owned()));
            map.insert(
                "op".to_owned(),
                Value::String("signature.verify".to_owned()),
            );
        }
        _ => {}
    }
}

fn positive_scan_metric(value: &mut Value, metric: &str) {
    match value {
        Value::Object(map) => {
            if map.get("$match").and_then(Value::as_str) == Some("explainScan") {
                if let Some(count) = map
                    .get_mut("metrics")
                    .and_then(Value::as_object_mut)
                    .and_then(|metrics| metrics.get_mut(metric))
                {
                    *count = serde_json::json!({ "$match": "positive" });
                }
                return;
            }
            for child in map.values_mut() {
                positive_scan_metric(child, metric);
            }
        }
        Value::Array(items) => {
            for item in items {
                positive_scan_metric(item, metric);
            }
        }
        _ => {}
    }
}

/// [`apply`] on the steps of a loaded scenario.
pub fn steps(
    id: &str,
    steps: &[integration_test::export::WireStep],
) -> (
    Vec<integration_test::export::WireStep>,
    Option<&'static str>,
) {
    let mut wire: Vec<Value> = steps
        .iter()
        .map(|step| serde_json::to_value(step).expect("a step serializes"))
        .collect();
    let reason = apply(id, &mut wire);
    let steps = wire
        .into_iter()
        .map(|step| serde_json::from_value(step).expect("an edited step parses"))
        .collect();
    (steps, reason)
}
