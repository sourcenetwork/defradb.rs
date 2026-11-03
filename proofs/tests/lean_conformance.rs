//! Lean axis: assert the Lean-generated contract still matches the live
//! Rust types. A drift in either side fails here. No binary needed.

use conformance::lean_contract::{load_contract, ContractSnapshot};

/// The Lean drift-check builds the contract via `lake`. Environments without a
/// Lean toolchain (default CI `cargo test --workspace`) can't run it, so skip
/// gracefully there; it still runs wherever `lake` is installed (local dev,
/// `proofs/verify-all.sh`, a Lean-equipped job).
fn lake_available() -> bool {
    std::process::Command::new("lake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Mirrors `crates/crdt/src/field/mod.rs` `enum MergeResult`. If a variant is
/// added/renamed there, update this list and `Conformance.lean` together — the
/// assertion is what forces that.
const MERGE_RESULT_VARIANTS: &[&str] = &["Applied", "RejectedLowerPriority", "RejectedTieBreak"];

/// Mirrors `crates/zanzibar/src/expression/mod.rs` `enum RelationExpression`.
const RELATION_EXPRESSION_VARIANTS: &[&str] = &[
    "This",
    "ComputedUserset",
    "TupleToUserset",
    "Union",
    "Intersection",
    "Difference",
];

#[test]
fn lean_merge_result_vocab_matches_rust() {
    if !lake_available() {
        eprintln!("skipping lean_merge_result_vocab_matches_rust: `lake` (Lean) not on PATH");
        return;
    }
    let snapshot: ContractSnapshot = load_contract().expect("load Lean conformance contract");
    let vocab = snapshot
        .vocab("MergeResult")
        .expect("MergeResult vocabulary present in Lean contract");
    assert_eq!(
        vocab.values, MERGE_RESULT_VARIANTS,
        "Lean MergeResult vocabulary drifted from Rust crdt::MergeResult variants"
    );
}

#[test]
fn lean_relation_expression_vocab_matches_rust() {
    if !lake_available() {
        eprintln!(
            "skipping lean_relation_expression_vocab_matches_rust: `lake` (Lean) not on PATH"
        );
        return;
    }
    let snapshot: ContractSnapshot = load_contract().expect("load Lean conformance contract");
    let vocab = snapshot
        .vocab("RelationExpression")
        .expect("RelationExpression vocabulary present in Lean contract");
    assert_eq!(
        vocab.values, RELATION_EXPRESSION_VARIANTS,
        "Lean RelationExpression vocabulary drifted from Rust zanzibar::RelationExpression variants"
    );
}

/// `proofs/lean/Arbitration/Constants.lean` `contract`: the estimators'
/// constants, whose side conditions the Lean proofs check on exactly these
/// values. Each string is formatted from the live Rust constant.
fn arbitration_constants() -> Vec<String> {
    use p2p::sync::arbitration::*;
    let millis = |duration: std::time::Duration| duration.as_millis();
    let table: Vec<String> = WEIGHT_TABLE.iter().map(u64::to_string).collect();
    vec![
        format!("VALUE_BITS={VALUE_BITS}"),
        format!("TIME_BITS={TIME_BITS}"),
        format!("WEIGHT_ONE={WEIGHT_ONE}"),
        format!("PROVIDER_HALF_LIFE_MS={}", millis(PROVIDER_HALF_LIFE)),
        format!("ROOT_SHIFT={ROOT_SHIFT}"),
        format!("OCCUPANCY_SHIFT={OCCUPANCY_SHIFT}"),
        format!("OCCUPANCY_TICK_MS={}", millis(OCCUPANCY_TICK)),
        format!("TIE_BUCKET_MS={}", millis(TIE_BUCKET)),
        format!("BACKOFF_FLOOR_MS={}", millis(BACKOFF_FLOOR)),
        format!("LATENCY_UNIT_MICROS={LATENCY_UNIT_MICROS}"),
        format!("SCALE_PPM={SCALE_PPM}"),
        format!("USEFUL_FLOOR_PPM={USEFUL_FLOOR_PPM}"),
        format!("WEIGHT_TABLE={}", table.join(",")),
    ]
}

#[test]
fn lean_arbitration_constants_match_rust() {
    if !lake_available() {
        eprintln!("skipping lean_arbitration_constants_match_rust: `lake` (Lean) not on PATH");
        return;
    }
    let snapshot: ContractSnapshot = load_contract().expect("load Lean conformance contract");
    let vocab = snapshot
        .vocab("ArbitrationConstants")
        .expect("ArbitrationConstants vocabulary present in Lean contract");
    assert_eq!(
        vocab.values,
        arbitration_constants(),
        "Lean Arbitration constants drifted from p2p::sync::arbitration"
    );
}
