//! Allocations per operation on the paths a request actually walks.
//!
//! ```text
//! cargo bench -p benches --bench allocs
//! ```
//!
//! A count, not a rate: a busy host cannot move it, so this stays comparable
//! on a runner the load guard failed and a change in it is always a change in
//! the code. That makes it the cheapest early warning the suite has for a
//! clone that crept into a hot path, long before the wall clock notices.
//!
//! What is counted is every allocation the process made while performing the
//! operations, divided by how many there were. That includes whatever the
//! runtime allocated on the way, which is why the runtime here is
//! single-threaded and why the number is reported as "observed per operation"
//! rather than as the operation's own footprint. The comparison between runs
//! is the point; the absolute figure is a ceiling, not an attribution.
//!
//! Not a criterion target: an allocation count is exact on one execution, and
//! sampling it a thousand times would only measure the sampler.

use std::sync::Arc;

use db::DB;
use defra_perf::emit::{Family, Group, Row};
use defra_perf::measure::{per_op, reset, taken, CountingAllocator};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use query::DocFetcher;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

/// Counts on top of the allocator every other bench runs on
/// (`common/allocator.rs`).
#[global_allocator]
static ALLOC: CountingAllocator<mimalloc::MiMalloc> = CountingAllocator(mimalloc::MiMalloc);

const COLLECTION: &str = "Users";
const FIELDS: usize = 16;
const OPS: u64 = 200;
/// How many collections one transaction loads into its catalog, per row. A
/// load that copied the whole catalog would grow with this; one that does not
/// stays flat across the rows.
const CATALOG_SIZES: [usize; 3] = [10, 50, 200];

fn field_names() -> Vec<String> {
    (0..FIELDS).map(|i| format!("field_{i}")).collect()
}

fn collection_version() -> CollectionVersion {
    let mut fields = vec![FieldDescription::new(
        "1",
        "_docID",
        FieldKind::doc_id(),
        schema::CType::LwwRegister,
    )];
    for (index, name) in field_names().iter().enumerate() {
        fields.push(FieldDescription::new(
            (index + 2).to_string(),
            name,
            FieldKind::string(),
            schema::CType::LwwRegister,
        ));
    }
    CollectionVersion::new(COLLECTION, "bafkallocs", "allocs", fields)
}

fn catalog_collection(index: usize) -> CollectionVersion {
    CollectionVersion::new(
        format!("Catalog{index}"),
        format!("bafkcatalog{index}"),
        format!("catalog{index}"),
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

fn document(seq: usize) -> Document {
    let mut doc = Document::new(collection_version());
    for name in field_names() {
        doc.set(&name, NormalValue::String(format!("{name}-{seq}")))
            .unwrap();
    }
    doc
}

fn main() {
    // Single-threaded on purpose: a worker pool allocates on threads this has
    // no way to attribute, and the resulting count would drift with the
    // scheduler rather than with the code.
    let rt = nuclei::Runtime::builder()
        .worker_threads(0)
        .build()
        .expect("a runtime");

    let mut group = Group::lower_better("per operation", "allocations/op").note(
        "Allocations observed across the whole process while performing the operation, divided \
         by how many were performed. A ceiling on the operation's own cost, not an attribution \
         of it.",
    );

    let store = Arc::new(RegolithStore::in_memory().expect("an in-memory store"));
    let db = Arc::new(rt.block_on(DB::open_from_arc(store)).expect("a database"));
    rt.block_on(db.create_collection(collection_version()))
        .expect("the collection");
    let mutator = db::write::autocommit::AutoCommitMutator::new(db.clone());
    let fetcher = db::AutoCommitFetcher::new(db.clone());

    // The document set both the write and the read rows work over. Seeded
    // before any counting starts so its cost lands in neither.
    let mut seeded = Vec::new();
    for seq in 0..OPS as usize {
        let created = rt
            .block_on(mutator.create(COLLECTION, document(seq)))
            .expect("the seed create to succeed");
        seeded.push(created.doc_id.to_string());
    }

    group = group.row(Row::new(
        "document create",
        per_op(OPS, || {
            for seq in 0..OPS as usize {
                rt.block_on(mutator.create(COLLECTION, document(OPS as usize + seq)))
                    .expect("the create to succeed");
            }
        }),
    ));

    group = group.row(Row::new(
        "document get by id",
        per_op(OPS, || {
            for doc_id in &seeded {
                rt.block_on(fetcher.get_by_ids(COLLECTION, std::slice::from_ref(doc_id)))
                    .expect("the read to succeed");
            }
        }),
    ));

    // Pure codec paths, with no store under them: these are the ones a change
    // in a serializer moves first.
    let doc = document(0);
    group = group.row(Row::new(
        "document to map",
        per_op(OPS, || {
            for _ in 0..OPS {
                std::hint::black_box(doc.to_map().expect("the document to serialize"));
            }
        }),
    ));

    let json = serde_json::to_vec(&doc.to_map().expect("the document to serialize"))
        .expect("a JSON encoding of it");
    // Built once, as real callers share one definition: the row counts the
    // parse, not a definition build per document.
    let collection = Arc::new(collection_version());
    group = group.row(Row::new(
        "document from json",
        per_op(OPS, || {
            for _ in 0..OPS {
                std::hint::black_box(
                    Document::from_json(&json, Arc::clone(&collection))
                        .expect("the document to parse"),
                );
            }
        }),
    ));

    // One transaction loading `size` collections, one at a time, as a request
    // does on first use of each. Per load, so a row that rises with `size` is
    // a load whose cost depends on how much the transaction already holds.
    let names: Vec<String> = (0..CATALOG_SIZES[CATALOG_SIZES.len() - 1])
        .map(|index| {
            let version = catalog_collection(index);
            let name = version.name.clone();
            rt.block_on(db.create_collection(version))
                .expect("the catalog collection");
            name
        })
        .collect();
    let mut catalog_bytes = Group::lower_better("collection load bytes", "bytes/op").note(
        "Bytes allocated across the whole process per collection a transaction loads into its \
         catalog, with that many collections loaded into the one transaction.",
    );
    for size in CATALOG_SIZES {
        let load = || {
            let txn = rt.block_on(db.new_txn(true)).expect("a transaction");
            for name in &names[..size] {
                rt.block_on(txn.get_collection(name))
                    .expect("the load to succeed")
                    .expect("the collection to exist");
            }
            txn.discard().expect("the discard to succeed");
        };
        load();
        reset();
        load();
        let counted = taken();
        let label = format!("collection load, {size} in txn");
        group = group.row(Row::new(&label, counted.count as f64 / size as f64));
        catalog_bytes = catalog_bytes.row(Row::new(&label, counted.bytes as f64 / size as f64));
    }

    let family = Family::new(
        "Allocations per operation",
        format!(
            "How many allocations each path makes, averaged over {OPS} operations. Deterministic: \
             a count does not move because the runner was busy, so this is comparable across \
             every run whatever the load guard said."
        ),
    )
    .deterministic()
    .group(group)
    .group(catalog_bytes);

    for group in &family.groups {
        for row in &group.rows {
            println!("  {:<32} {:>12.1} {}", row.name, row.value, group.unit);
        }
    }
    family.emit("allocs");
}
