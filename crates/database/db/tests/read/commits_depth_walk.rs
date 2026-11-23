//! The walk `_commits(cid:, depth:)` makes below its commit: each commit once
//! per owner, last head first, with no row a shorter path reaches lost.

use std::sync::Arc;

use cid::Cid;
use defra_core::{Block, CrdtDelta, LwwDeltaPayload};
use document::{DocID, NormalValue};
use storage::RegolithStore;

use db::read::commits::{CommitsFetcher, CommitsQueryOptions};
use db::DB;

use crate::common::counting_store::CountingStore;

fn block(value: &str, priority: u64, heads: Vec<Cid>) -> Block {
    let mut data = Vec::new();
    ciborium::into_writer(&NormalValue::String(value.to_string()), &mut data).unwrap();
    Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            collection_version_id: "v1".to_string(),
            priority,
            data,
        }),
        heads,
        vec![],
    )
}

/// The blocks of one document, put in a store in one transaction.
struct History {
    blocks: Vec<(Cid, Block)>,
}

impl History {
    fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    fn add(&mut self, value: &str, priority: u64, heads: Vec<Cid>) -> Cid {
        self.add_block(block(value, priority, heads))
    }

    fn add_block(&mut self, block: Block) -> Cid {
        let cid = block.generate_cid().unwrap();
        self.blocks.push((cid, block));
        cid
    }

    /// The heads of `cid` in the order its block stores them (sorted by CID
    /// text), which is the order the walk pushes them in.
    fn heads(&self, cid: Cid) -> Vec<Cid> {
        let (_, block) = self.blocks.iter().find(|(c, _)| *c == cid).unwrap();
        block.heads.clone().unwrap_or_default()
    }

    /// Stores the blocks and returns the CIDs `_commits(cid: top, depth:)`
    /// lists, in order.
    fn walk(&self, top: Cid, depth: u64) -> Vec<Cid> {
        self.walk_counted(top, depth).0
    }

    /// `walk`, plus how many point reads of block keys the walk made.
    fn walk_counted(&self, top: Cid, depth: u64) -> (Vec<Cid>, usize) {
        workers::block_on(async {
            let db =
                Arc::new(DB::new(CountingStore::new(RegolithStore::in_memory().unwrap())).unwrap());
            let doc_id =
                DocID::new_v0(defra_core::block::generate_cid_from_bytes(b"owner").unwrap())
                    .to_string();
            let txn = db.new_txn(false).await.unwrap();
            {
                let blockstore = txn.blockstore().unwrap();
                let systemstore = txn.systemstore().unwrap();
                for (cid, block) in &self.blocks {
                    blockstore
                        .set(&cid.to_bytes(), &block.to_dag_cbor().unwrap())
                        .await
                        .unwrap();
                }
                db::docid::map::set_block_doc_id_mapping(&systemstore, &top.to_string(), &doc_id)
                    .await
                    .unwrap();
            }
            txn.commit().await.unwrap();

            db.store().take_point_keys();
            let read = db.new_txn(true).await.unwrap();
            let rows = CommitsFetcher::new(Arc::new(read))
                .fetch_commits(&CommitsQueryOptions {
                    cid: Some(top.to_string()),
                    depth: Some(depth),
                    ..Default::default()
                })
                .await
                .unwrap();
            let block_keys: Vec<Vec<u8>> =
                self.blocks.iter().map(|(cid, _)| cid.to_bytes()).collect();
            let block_reads = db
                .store()
                .take_point_keys()
                .iter()
                .filter(|key| block_keys.iter().any(|block| key.ends_with(block)))
                .count();
            let rows = rows
                .iter()
                .map(|row| {
                    assert_eq!(
                        row.get("docID").and_then(NormalValue::as_str),
                        Some(doc_id.as_str())
                    );
                    row.get("cid")
                        .and_then(NormalValue::as_str)
                        .unwrap()
                        .parse()
                        .unwrap()
                })
                .collect();
            (rows, block_reads)
        })
        .unwrap()
    }
}

/// `k` diamonds stacked: `bottoms[i]` has two heads, both of which have head
/// `bottoms[i - 1]`.
fn diamond_chain(k: usize) -> (History, Vec<Cid>) {
    let mut history = History::new();
    let mut bottoms = vec![history.add("b0", 1, vec![])];
    for i in 1..=k {
        let below = bottoms[i - 1];
        let height = 2 * i as u64;
        let left = history.add(&format!("l{i}"), height, vec![below]);
        let right = history.add(&format!("r{i}"), height, vec![below]);
        bottoms.push(history.add(&format!("b{i}"), height + 1, vec![left, right]));
    }
    (history, bottoms)
}

#[test]
fn commits_depth_walk_lists_each_commit_of_a_diamond_chain_once_last_head_first() {
    let k = 6;
    let (history, bottoms) = diamond_chain(k);

    let rows = history.walk(bottoms[k], u64::MAX);

    // Go's DAG scan pops the last head first and marks a commit when it is
    // popped: down each diamond's last head to the root, then each first
    // head from the bottom up.
    let mut expected = vec![bottoms[k]];
    let mut firsts = Vec::new();
    for i in (1..=k).rev() {
        let heads = history.heads(bottoms[i]);
        expected.push(heads[1]);
        expected.push(bottoms[i - 1]);
        firsts.push(heads[0]);
    }
    expected.extend(firsts.into_iter().rev());
    assert_eq!(rows, expected);
}

#[test]
fn commits_depth_walk_keeps_rows_a_shorter_path_reaches() {
    // top -> [short, long]; short -> shared; long -> mid -> shared;
    // shared -> deep -> deeper. `depth: 5` lists what lies 4 heads below
    // `top`: `deeper` only through `short`, though `long`, the last head
    // (heads are stored sorted by CID text), is walked first.
    let mut history = History::new();
    let deeper = history.add("deeper", 1, vec![]);
    let deep = history.add("deep", 2, vec![deeper]);
    let shared = history.add("shared", 3, vec![deep]);
    let mid = history.add("mid", 4, vec![shared]);
    let short = history.add("short", 4, vec![shared]);
    let long = (0..)
        .map(|n| block(&format!("long{n}"), 5, vec![mid]))
        .find(|long| long.generate_cid().unwrap().to_string() > short.to_string())
        .unwrap();
    let long = history.add_block(long);
    let top = history.add("top", 6, vec![short, long]);
    assert_eq!(history.heads(top), vec![short, long]);

    let rows = history.walk(top, 5);

    assert_eq!(rows, vec![top, long, mid, shared, deep, short, deeper]);
}

#[test]
fn commits_depth_walk_lists_a_long_history_at_unlimited_depth() {
    let updates = 2_000;
    let mut history = History::new();
    let mut cids = vec![history.add("v0", 1, vec![])];
    for i in 1..=updates {
        let head = cids[i - 1];
        cids.push(history.add(&format!("v{i}"), i as u64 + 1, vec![head]));
    }

    let rows = history.walk(cids[updates], u64::MAX);

    cids.reverse();
    assert_eq!(rows.len(), updates + 1);
    assert_eq!(rows, cids);
}

/// `k` merges stacked: `tops[i]` has two heads, a one-commit branch and a
/// `long`-commit branch, both on `tops[i - 1]`. With `long_last` the long
/// branch is stored last, so the walk takes it first and later meets
/// `tops[i - 1]` again, with more depth left, through the short branch;
/// without it the short branch is taken first and nothing is met again.
fn ladder(k: usize, long: usize, long_last: bool) -> (History, Vec<Cid>) {
    let mut history = History::new();
    let mut tops = vec![history.add("ladder-root", 1, vec![])];
    for i in 1..=k {
        let below = tops[i - 1];
        let height = ((long + 1) * (i - 1) + 2) as u64;
        let short = history.add(&format!("ladder-s{i}"), height, vec![below]);
        let mut chain = below;
        for j in 0..long - 1 {
            chain = history.add(&format!("ladder-x{i}-{j}"), height + j as u64, vec![chain]);
        }
        let last = (0..)
            .map(|n| {
                block(
                    &format!("ladder-x{i}-top{n}"),
                    height + long as u64,
                    vec![chain],
                )
            })
            .find(|last| {
                (last.generate_cid().unwrap().to_string() > short.to_string()) == long_last
            })
            .unwrap();
        let last = history.add_block(last);
        let top = history.add(
            &format!("ladder-t{i}"),
            height + long as u64 + 1,
            vec![short, last],
        );
        assert_eq!(history.heads(top)[usize::from(long_last)], last);
        tops.push(top);
    }
    (history, tops)
}

#[test]
fn commits_depth_walk_reads_each_block_once_at_unlimited_depth() {
    // At unlimited depth a commit reached again with more depth left leads to
    // nothing new, so walking it again only reads blocks again. Long branch
    // first, each short branch reaches the merge below with more depth than
    // the long branch did: re-walking it reads every block below each merge
    // again, quadratic in the merges. Short branch first, the same shape
    // meets nothing twice, so it is the count to match.
    let k = 40;
    let (long_first, long_first_tops) = ladder(k, 10, true);
    let (short_first, short_first_tops) = ladder(k, 10, false);
    let blocks = long_first.blocks.len();

    // `depth: -1` parses to u64::MAX.
    let (rows, long_first_reads) = long_first.walk_counted(long_first_tops[k], u64::MAX);
    let (_, short_first_reads) = short_first.walk_counted(short_first_tops[k], u64::MAX);

    let unique: rapidhash::RapidHashSet<Cid> = rows.iter().copied().collect();
    assert_eq!(rows.len(), blocks);
    assert_eq!(unique.len(), blocks);
    assert_eq!(
        long_first_reads, short_first_reads,
        "walking each long branch first reads no block more often"
    );
}
