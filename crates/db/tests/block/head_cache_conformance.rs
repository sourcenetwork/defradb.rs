//! Executes the same transaction actions in HeadSet.Core and the storage owner.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

use cid::Cid;
use datastore::{NamespaceView, SharedTxn};
use db::block::heads::{live_collection_heads, prune_superseded_heads, record_supersedes};
use defra_core::block::generate_cid_from_bytes;
use serde::Serialize;
use serde_json::{json, Value};
use storage::corekv::{Key, Store};
use storage::keys::headstore::HeadstoreColKey;
use storage::namespace::Namespace;
use storage::RegolithStore;

#[derive(Clone, Serialize)]
struct Action {
    op: &'static str,
    txn: u32,
    block: u32,
    parents: Vec<u32>,
}

fn action(op: &'static str, txn: u32) -> Action {
    Action {
        op,
        txn,
        block: 0,
        parents: vec![],
    }
}

fn append(txn: u32, block: u32, parents: &[u32]) -> Action {
    Action {
        op: "append",
        txn,
        block,
        parents: parents.to_vec(),
    }
}

fn cid(block: u32) -> Cid {
    generate_cid_from_bytes(&block.to_be_bytes()).unwrap()
}

fn cases() -> Vec<Vec<Action>> {
    let seed = vec![action("begin", 0), append(0, 1, &[]), action("commit", 0)];
    let mut cases = vec![
        // Warm the cache, commit siblings from the same snapshot in both orders.
        vec![
            action("begin", 1),
            action("read", 1),
            action("begin", 2),
            action("read", 2),
            append(1, 2, &[1]),
            append(2, 3, &[1]),
            action("commit", 1),
            action("commit", 2),
            action("begin", 3),
            action("read", 3),
        ],
        vec![
            action("begin", 1),
            action("read", 1),
            action("begin", 2),
            action("read", 2),
            append(1, 2, &[1]),
            append(2, 3, &[1]),
            action("commit", 2),
            action("commit", 1),
            action("begin", 3),
            action("read", 3),
        ],
        // A cold reader must not publish its stale snapshot over a newer commit.
        vec![
            action("begin", 1),
            action("begin", 2),
            append(2, 2, &[1]),
            action("commit", 2),
            action("read", 1),
            action("begin", 3),
            action("read", 3),
        ],
        // A cold fill excludes pending writes, including ones later discarded.
        vec![
            action("begin", 1),
            append(1, 2, &[1]),
            action("read", 1),
            action("begin", 2),
            action("read", 2),
            action("abort", 1),
            action("read", 2),
        ],
        // Own writes, overwrite/replay, and multiple appends in one transaction.
        vec![
            action("begin", 1),
            action("read", 1),
            append(1, 2, &[1]),
            action("read", 1),
            append(1, 3, &[2]),
            append(1, 3, &[2]),
            action("read", 1),
            action("commit", 1),
            action("begin", 2),
            action("read", 2),
        ],
        // Pruning cannot change old snapshots or abort an in-flight append.
        vec![
            action("begin", 1),
            append(1, 2, &[1]),
            action("commit", 1),
            action("begin", 2),
            action("read", 2),
            action("begin", 3),
            action("prune", 3),
            action("commit", 3),
            action("read", 2),
            append(2, 3, &[2]),
            action("commit", 2),
            action("begin", 4),
            action("read", 4),
        ],
        // A parent arriving after its child must still be superseded.
        vec![
            action("begin", 1),
            append(1, 3, &[2]),
            action("commit", 1),
            action("begin", 2),
            action("read", 2),
            action("abort", 2),
            action("begin", 3),
            append(3, 2, &[1]),
            action("read", 3),
            action("commit", 3),
            action("begin", 4),
            action("prune", 4),
            action("commit", 4),
            action("begin", 5),
            action("read", 5),
        ],
        // A prune only deletes the markers it observed, not a late sibling's.
        vec![
            action("begin", 1),
            append(1, 2, &[1]),
            action("commit", 1),
            action("begin", 2),
            action("read", 2),
            action("prune", 2),
            action("begin", 3),
            append(3, 3, &[1]),
            action("commit", 3),
            action("commit", 2),
            action("begin", 4),
            action("read", 4),
        ],
    ];
    for case in &mut cases {
        case.splice(0..0, seed.clone());
    }
    cases
}

#[tokio::test]
async fn head_queries_match_executable_lean() {
    if Command::new("lake").arg("--version").output().is_err() {
        eprintln!(
            "head_queries_match_executable_lean requires lake; run the proof conformance gate"
        );
        return;
    }
    let lean_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs/lean");
    let built = Command::new("lake")
        .current_dir(&lean_dir)
        .args(["build", "HeadSet"])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for (index, actions) in cases().into_iter().enumerate() {
        let mut oracle = Command::new("lake")
            .current_dir(&lean_dir)
            .args(["env", "lean", "--run", "HeadSet/Conformance.lean"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            oracle.stdin.take().unwrap(),
            "{}",
            serde_json::to_string(&actions).unwrap()
        )
        .unwrap();
        let output = oracle.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        let store = RegolithStore::in_memory().unwrap();
        let mut txns: BTreeMap<u32, Arc<SharedTxn>> = BTreeMap::new();
        let mut actual = vec![];
        for a in actions {
            match a.op {
                "begin" => {
                    txns.insert(a.txn, SharedTxn::new(store.new_txn(false).await.unwrap()));
                }
                "commit" | "abort" => {
                    let txn = Arc::try_unwrap(txns.remove(&a.txn).unwrap())
                        .ok()
                        .unwrap()
                        .into_txn();
                    if a.op == "commit" {
                        txn.commit().await.unwrap();
                    } else {
                        txn.discard();
                    }
                }
                _ => {
                    let view = NamespaceView::new(txns[&a.txn].clone(), Namespace::Headstore);
                    match a.op {
                        "append" => {
                            let parents: Vec<_> = a.parents.into_iter().map(cid).collect();
                            record_supersedes(&view, 3, &parents, cid(a.block))
                                .await
                                .unwrap();
                            view.set(
                                &HeadstoreColKey::new(3, cid(a.block)).bytes(),
                                &[(a.block + 1) as u8],
                            )
                            .await
                            .unwrap();
                        }
                        "prune" => {
                            prune_superseded_heads(&view, 3, usize::MAX).await.unwrap();
                        }
                        "read" => {
                            let found = live_collection_heads(&view, 3).await.unwrap();
                            let mut heads: Vec<_> = found
                                .live
                                .iter()
                                .map(|head| (0..64).find(|b| cid(*b) == *head).unwrap())
                                .collect();
                            heads.sort_unstable();
                            actual.push(json!({"heads": heads, "superseded": found.superseded, "max_priority": found.max_priority}));
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        assert_eq!(actual, expected, "Lean action trace {index}");
    }
}
