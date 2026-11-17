//! Runs operation traces through `DeferredMerges` and through
//! `proofs/lean/DeferredMerges/Conformance.lean`, and compares what each
//! reports after every operation.

use std::io::Write as _;
use std::process::{Command, Stdio};

use cid::Cid;
use defra_core::merge::MergeBlock;
use rapidhash::RapidHashMap;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::Digest as _;

use super::deferred::{
    DeferredMerges, MAX_AWAITED_PER_COMPOSITE, MAX_DEFERRED_COMPOSITES, MAX_WAITERS_PER_DEPENDENCY,
};
use super::WaitKey;

#[derive(Clone, Serialize)]
struct KeyIn {
    kind: &'static str,
    n: u64,
}

#[derive(Clone, Serialize)]
struct OpIn {
    op: &'static str,
    cid: u64,
    keys: Vec<KeyIn>,
    doc: u64,
    refused: u64,
    n: u64,
}

impl OpIn {
    fn new(op: &'static str) -> Self {
        Self {
            op,
            cid: 0,
            keys: Vec::new(),
            doc: 0,
            refused: 0,
            n: 0,
        }
    }
}

#[derive(Serialize)]
struct TraceIn {
    capacity: usize,
    ops: Vec<OpIn>,
}

fn cid(n: u64) -> Cid {
    use cid::multihash::Multihash;
    let digest = sha2::Sha256::digest(n.to_le_bytes());
    Cid::new_v1(0x71, Multihash::wrap(0x12, &digest).unwrap())
}

fn block(n: u64) -> MergeBlock {
    MergeBlock {
        cid: cid(n),
        block_data: Default::default(),
        doc_id: String::new(),
        collection_id: String::new(),
        creator: String::new(),
        sender_peer: None,
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    }
}

fn key(k: &KeyIn) -> WaitKey {
    match k.kind {
        "composite" => WaitKey::Composite(cid(k.n)),
        "field" => WaitKey::ImmutableField {
            collection: "c".into(),
            field: "f".into(),
            value: k.n.to_le_bytes().to_vec(),
        },
        "acp" => WaitKey::acp_object("p", "r", &format!("doc-{}", k.n)),
        other => panic!("unknown key kind {other}"),
    }
}

fn park_keys(doc: u64, refused: u64) -> Vec<WaitKey> {
    vec![
        WaitKey::acp_object("p", "r", &format!("doc-{doc}")),
        WaitKey::Composite(cid(refused)),
    ]
}

/// Apply one operation to the Rust index and report what the model reports.
fn apply(index: &DeferredMerges, names: &RapidHashMap<Cid, u64>, op: &OpIn) -> Value {
    let result = match op.op {
        "defer" => {
            index.defer(block(op.cid), op.keys.iter().map(key).collect());
            Value::Null
        }
        "park" => {
            index.defer(block(op.cid), park_keys(op.doc, op.refused));
            Value::Null
        }
        "fill" => {
            for c in op.cid..op.cid + op.n {
                index.defer(block(c), park_keys(op.doc, c));
            }
            Value::Null
        }
        "release" => {
            index.release(op.keys.iter().map(key));
            Value::Null
        }
        "releaseParked" => json!(index.release_parked(op.n as usize)),
        "enqueue" => json!(index.enqueue_ready(block(op.cid))),
        "forget" => json!(index.forget(&cid(op.cid))),
        "drain" => {
            let mut taken = Vec::new();
            while let Some(block) = index.take_ready() {
                taken.push(names[&block.cid]);
            }
            taken.sort_unstable();
            json!(taken)
        }
        other => panic!("unknown operation {other}"),
    };
    json!({"result": result, "len": index.len(), "has_waiters": index.has_waiters()})
}

/// A small deterministic generator, so a failing trace can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % bound
    }
}

fn random_key(rng: &mut Rng) -> KeyIn {
    match rng.next(3) {
        0 => KeyIn {
            kind: "composite",
            n: rng.next(12),
        },
        1 => KeyIn {
            kind: "field",
            n: rng.next(3),
        },
        _ => KeyIn {
            kind: "acp",
            n: rng.next(4),
        },
    }
}

/// A trace mixing every operation over a few composites, documents and
/// field values. The sweep releases every parked document or none, since
/// which of several it picks follows the map's order.
fn random_trace(seed: u64, capacity: usize) -> TraceIn {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut ops = Vec::new();
    for _ in 0..80 {
        let mut op = match rng.next(10) {
            0 | 1 => {
                let mut op = OpIn::new("park");
                op.doc = rng.next(4);
                op.refused = rng.next(12);
                op
            }
            2 | 3 => {
                let mut op = OpIn::new("defer");
                op.keys = (0..rng.next(4)).map(|_| random_key(&mut rng)).collect();
                op
            }
            4 | 5 => {
                let mut op = OpIn::new("release");
                op.keys = (0..1 + rng.next(2)).map(|_| random_key(&mut rng)).collect();
                op
            }
            6 => {
                let mut op = OpIn::new("releaseParked");
                op.n = if rng.next(2) == 0 { 0 } else { 1_000 };
                op
            }
            7 => OpIn::new("enqueue"),
            8 => OpIn::new("forget"),
            _ => OpIn::new("drain"),
        };
        op.cid = rng.next(12);
        ops.push(op);
    }
    ops.push(OpIn::new("drain"));
    TraceIn { capacity, ops }
}

/// More composites parked under one document than it holds, then released
/// by the document's change and the sweep.
fn document_bound_trace() -> TraceIn {
    let mut fill = OpIn::new("fill");
    fill.cid = 100;
    fill.doc = 1;
    fill.n = MAX_WAITERS_PER_DEPENDENCY as u64 + 8;
    let mut park = OpIn::new("park");
    park.cid = 5;
    park.doc = 1;
    park.refused = 6;
    let mut sweep = OpIn::new("releaseParked");
    sweep.n = 1_000;
    let mut merged = OpIn::new("release");
    merged.keys = vec![KeyIn {
        kind: "composite",
        n: 6,
    }];
    let mut forget = OpIn::new("forget");
    forget.cid = 100 + MAX_WAITERS_PER_DEPENDENCY as u64;
    TraceIn {
        capacity: MAX_DEFERRED_COMPOSITES,
        ops: vec![
            fill,
            park,
            sweep,
            OpIn::new("drain"),
            merged,
            OpIn::new("drain"),
            forget,
        ],
    }
}

#[test]
#[ignore = "requires Lean; executed explicitly by the Lean Conformance CI job"]
fn the_index_matches_the_executed_model() {
    let mut traces: Vec<TraceIn> = (1..=60)
        .map(|seed| {
            random_trace(
                seed,
                if seed % 3 == 0 {
                    4
                } else {
                    MAX_DEFERRED_COMPOSITES
                },
            )
        })
        .collect();
    traces.push(document_bound_trace());

    let mut oracle = Command::new("lake")
        .args(["env", "lean", "--run", "DeferredMerges/Conformance.lean"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("execute the index model");
    writeln!(
        oracle.stdin.take().unwrap(),
        "{}",
        serde_json::to_string(&traces).unwrap()
    )
    .unwrap();
    let output = oracle.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let model: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        model["limits"],
        json!([
            MAX_DEFERRED_COMPOSITES,
            MAX_WAITERS_PER_DEPENDENCY,
            MAX_AWAITED_PER_COMPOSITE
        ]),
        "the model's bounds are the Rust constants"
    );

    let names: RapidHashMap<Cid, u64> = (0..2_000).map(|n| (cid(n), n)).collect();
    let runs = model["runs"].as_array().unwrap();
    assert_eq!(runs.len(), traces.len());
    for (index_of, (trace, expected)) in traces.iter().zip(runs).enumerate() {
        let index = DeferredMerges::default();
        if trace.capacity != MAX_DEFERRED_COMPOSITES {
            index.set_capacity(trace.capacity);
        }
        let expected = expected.as_array().unwrap();
        assert_eq!(expected.len(), trace.ops.len());
        for (step, (op, expected)) in trace.ops.iter().zip(expected).enumerate() {
            let actual = apply(&index, &names, op);
            assert_eq!(
                &actual, expected,
                "trace {index_of}, step {step} ({})",
                op.op
            );
        }
    }
}
