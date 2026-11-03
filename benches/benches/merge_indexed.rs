//! Merging a peer's edits of a JSON field, with an index on the field and
//! without one. The field merge writes the merged value into the document,
//! so an indexed merge also has to read the value from before it.

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
use schema::{
    CType, CollectionVersion, FieldDescription, FieldKind, IndexKind, IndexedFieldDescription,
    OrderedIndexDescription,
};
use storage::RegolithStore;
use storage::Store;

mod common;

const COLLECTION: &str = "Notes";
const COLLECTION_ID: &str = "col-notes";
const CREATOR: &str = "did:key:z6MkrMergeIndexedBench";
const DEPTH: usize = 16;

type BenchHandler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        COLLECTION,
        "v1",
        COLLECTION_ID,
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "meta", FieldKind::json(), CType::Json),
            FieldDescription::new("3", "title", FieldKind::string(), CType::LwwRegister),
        ],
    )
}

fn document(revision: usize) -> Document {
    let mut doc = Document::new(collection());
    doc.set(
        "meta",
        NormalValue::Json(serde_json::json!({ "rev": revision, "tag": "note" })),
    )
    .unwrap();
    doc.set("title", NormalValue::String(format!("note {revision}")))
        .unwrap();
    doc
}

/// A composite tip and every block below it.
struct Dag {
    tip: (Cid, Bytes),
    doc_id: String,
    blocks: Vec<(Cid, Bytes)>,
}

/// A chain of `DEPTH` revisions, each editing the JSON field, written by the
/// real write path on a throwaway store.
async fn build_dag() -> Dag {
    let txn = RegolithStore::in_memory()
        .unwrap()
        .new_txn(false)
        .await
        .unwrap();
    let shared = SharedTxn::new(txn, storage::Crdts::default());
    let blockstore = NamespaceView::new(shared.clone(), Namespace::Blockstore);
    let headstore = NamespaceView::new(shared.clone(), Namespace::Headstore);
    let identity = db::block::builder::DocRef::new(1, 1);
    let modified: RapidHashSet<String> = ["meta".to_owned()].into_iter().collect();
    let collection = collection();

    let mut result = db::block::builder::write_document_blocks(
        &blockstore,
        &headstore,
        &document(0),
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
    let mut cids = vec![result.cid];
    cids.extend(result.field_cids.iter().copied());
    for revision in 1..DEPTH {
        let mut doc = document(revision);
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
        let bytes = blockstore.get(&cid.to_bytes()).await.unwrap().unwrap();
        blocks.push((*cid, bytes));
    }
    Dag {
        tip: (result.cid, result.block),
        doc_id,
        blocks,
    }
}

/// A node holding `dag`'s blocks, none merged, with an index on the JSON
/// field or without one.
async fn seed(dag: &Dag, indexed: bool) -> BenchHandler {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(collection()).await.unwrap();
    if indexed {
        db.create_index(
            COLLECTION,
            Some("by_meta"),
            vec![IndexedFieldDescription {
                name: "meta".to_owned(),
                descending: false,
            }],
            IndexKind::Ordered(OrderedIndexDescription { unique: false }),
        )
        .await
        .unwrap();
    }
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    for (cid, bytes) in &dag.blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }
    DbMergeHandler::new(db, blockstore)
}

fn bench_merge_indexed(c: &mut Criterion) {
    let rt = runtime();
    let dag = rt.block_on(build_dag());
    let mut group = c.benchmark_group("merge_indexed");
    group.sample_size(20);
    group.throughput(Throughput::Elements(DEPTH as u64));

    for indexed in [false, true] {
        let label = if indexed {
            "json_indexed"
        } else {
            "json_plain"
        };
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            // `_ref`: dropping the node stays outside the measurement.
            b.iter_batched_ref(
                || rt.block_on(seed(&dag, indexed)),
                |handler| {
                    rt.block_on(async {
                        let metadata =
                            BlockMetadata::normal(&dag.doc_id, COLLECTION_ID, CREATOR, None, false);
                        let outcome = handler
                            .handle_block(&dag.tip.0, &dag.tip.1, metadata)
                            .await
                            .unwrap();
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
    group.finish();
}

criterion_group!(benches, bench_merge_indexed);
criterion_main!(benches);
