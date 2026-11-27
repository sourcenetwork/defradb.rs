//! Merge-side DAG walk benchmarks.
//!
//! Times [`DbMergeHandler::handle_block`] on a composite tip whose ancestry is
//! present in the blockstore but unmerged, which drives
//! `process_composite_delta` -> `prepare_composite_merge` -> head advance in
//! `composite_heads.rs`. Parameterized by DAG depth and per-composite field
//! link count. The `off_version_lens_` rows merge blocks at a version that a
//! patch and a pass-through migration left behind an indexed active version,
//! the path where a merge owes its collection a reindex.
//!
//! The ancestry is produced by the real write path (`write_document_blocks` on a
//! separate source store) and copied into the target blockstore, so the blocks
//! are wire-identical to replicated ones.
//!
//! The merge-depth policy caps the walk; the depths benched here stay under the
//! default so the measurement is of the walk itself rather than the guard.

use bytes::Bytes;
use rapidhash::RapidHashSet;
use std::hint::black_box;
use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use datastore::{Namespace, NamespaceView, SharedTxn};
use db::merge::DbMergeHandler;
use db::DB;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use lens::{LensConfig, LensModule, MemoryTransformStore};
use schema::CollectionVersion;
use storage::RegolithStore;
use storage::Store;

mod common;

const CREATOR: &str = "did:key:z6MkrMergeDagBench";
const DEPTHS: [usize; 3] = [1, 8, 64];
const FIELD_COUNTS: [usize; 2] = [1, 4];

type BenchHandler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

fn field_names(field_count: usize) -> Vec<String> {
    (0..field_count).map(|i| format!("field_{i}")).collect()
}

/// The collection's SDL, with an index on `field_0` when `indexed`. An index
/// does not change the version ID, so the blocks built from the plain version
/// merge into either.
fn collection_sdl(field_count: usize, indexed: bool) -> String {
    let fields = field_names(field_count)
        .iter()
        .map(|name| match (indexed, name.as_str()) {
            (true, "field_0") => format!(r#"{name}: String @index(name: "idx_field_0")"#),
            _ => format!("{name}: String"),
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("type Users {{ {fields} }}")
}

fn collection_version(field_count: usize) -> CollectionVersion {
    query::parse_sdl(&collection_sdl(field_count, false))
        .expect("the benchmark schema to parse")
        .remove(0)
}

/// Where the merged blocks' version `v1` stands against the active version.
#[derive(Clone, Copy)]
enum Placement {
    /// `v1` is active: the merge stores the document where reads see it.
    OnActive,
    /// `v1` was patched and migrated, through a pass-through lens, onto an
    /// indexed active version: the merge stores the document off the active
    /// version across a lens and owes the collection a reindex.
    OffActiveAcrossLens,
}

impl Placement {
    fn label(self) -> &'static str {
        match self {
            Placement::OnActive => "",
            Placement::OffActiveAcrossLens => "off_version_lens_",
        }
    }
}

async fn make_handler(
    field_count: usize,
    placement: Placement,
) -> (BenchHandler, Arc<DefraBlockstore<RegolithStore>>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let mut raw = DB::from_arc(store.clone()).unwrap();
    raw.set_lens_store(Arc::new(MemoryTransformStore::new()));
    let db = Arc::new(raw);
    let indexed = matches!(placement, Placement::OffActiveAcrossLens);
    let v1 = db
        .add_collection(
            &defra_core::RequestContext::ambient(),
            &collection_sdl(field_count, indexed),
        )
        .await
        .unwrap()
        .remove(0)
        .version_id;
    if let Placement::OffActiveAcrossLens = placement {
        db.patch_collection(
            &defra_core::RequestContext::default(),
            r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"level","Kind":"Int"}}]"#,
            None,
        )
        .await
        .unwrap();
        let v2 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();
    }
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db, blockstore.clone());
    (handler, blockstore)
}

fn make_document(collection: &CollectionVersion, field_count: usize, revision: usize) -> Document {
    let mut doc = Document::new(collection.clone());
    for name in field_names(field_count) {
        doc.set(&name, NormalValue::String(format!("{name}-rev{revision}")))
            .unwrap();
    }
    doc
}

/// A composite tip plus every ancestor block needed to merge it.
struct SyntheticDag {
    tip_cid: Cid,
    tip_bytes: Bytes,
    collection_id: String,
    doc_id: String,
    blocks: Vec<(Cid, Bytes)>,
}

/// Build a linear composite chain of `depth` revisions on a throwaway store,
/// then collect every block so it can be replayed into a fresh handler.
async fn build_dag(depth: usize, field_count: usize) -> SyntheticDag {
    let txn = RegolithStore::in_memory()
        .unwrap()
        .new_txn(false)
        .await
        .unwrap();
    let shared = SharedTxn::new(txn, storage::Crdts::default());
    let blockstore = NamespaceView::new(shared.clone(), Namespace::Blockstore);
    let headstore = NamespaceView::new(shared.clone(), Namespace::Headstore);
    let identity = db::block::builder::DocRef::new(1, 1);
    let modified: RapidHashSet<String> = field_names(field_count).into_iter().collect();
    let collection = collection_version(field_count);

    let mut doc = make_document(&collection, field_count, 0);
    let mut result = db::block::builder::write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        &collection,
        identity,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let doc_id = result.doc_id.clone();
    let mut cids: Vec<Cid> = vec![result.cid];
    cids.extend(result.field_cids.iter().copied());

    for revision in 1..depth {
        doc = make_document(&collection, field_count, revision);
        doc.set_id(DocID::from_string(&doc_id).unwrap());
        result = db::block::builder::write_document_blocks(
            &blockstore,
            &headstore,
            &doc,
            &collection,
            identity,
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        cids.push(result.cid);
        cids.extend(result.field_cids.iter().copied());
    }

    let mut blocks = Vec::with_capacity(cids.len());
    for cid in &cids {
        let bytes = blockstore
            .get(&cid.to_bytes())
            .await
            .unwrap()
            .expect("block written by the write path must be readable");
        blocks.push((*cid, bytes));
    }

    SyntheticDag {
        tip_cid: result.cid,
        tip_bytes: result.block,
        collection_id: collection.collection_id,
        doc_id,
        blocks,
    }
}

/// A fresh handler whose blockstore holds `dag`'s blocks but which has merged none
/// of them, so `handle_block` on the tip performs the full walk.
async fn seed_handler(
    dag: &SyntheticDag,
    field_count: usize,
    placement: Placement,
) -> BenchHandler {
    let (handler, blockstore) = make_handler(field_count, placement).await;
    for (cid, bytes) in &dag.blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }
    handler
}

fn bench_merge_dag(c: &mut Criterion) {
    let rt = common::runtime(1);
    let mut group = c.benchmark_group("merge_dag");
    group.sample_size(10);

    let rows = FIELD_COUNTS
        .into_iter()
        .map(|field_count| (Placement::OnActive, field_count))
        .chain([(Placement::OffActiveAcrossLens, 1)]);
    for (placement, field_count) in rows {
        for depth in DEPTHS {
            let dag = rt.block_on(build_dag(depth, field_count));
            let label = format!("{}depth{depth}_fields{field_count}", placement.label());
            group.throughput(Throughput::Elements(depth as u64));

            group.bench_function(BenchmarkId::from_parameter(&label), |b| {
                // `_ref`: the handler owns the DB and blockstore holding every block in
                // the DAG, so moving it into the routine would put its teardown inside
                // the measurement.
                b.iter_batched_ref(
                    || rt.block_on(seed_handler(&dag, field_count, placement)),
                    |handler| {
                        rt.block_on(async {
                            let metadata = BlockMetadata::normal(
                                &dag.doc_id,
                                &dag.collection_id,
                                CREATOR,
                                None,
                                false,
                            );
                            let outcome = handler
                                .handle_block(&dag.tip_cid, &dag.tip_bytes, metadata)
                                .await
                                .unwrap();
                            // A skipped or rejected block does no walk, so timing it
                            // would report a merge cost that was never paid.
                            assert!(
                                matches!(outcome, MergeOutcome::Merged),
                                "tip was not merged: {outcome:?}"
                            );
                            black_box(outcome)
                        })
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_merge_dag);
criterion_main!(benches);
