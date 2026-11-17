use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use db::{AutoCommitFetcher, AutoCommitMutator, DB};
use defra_core::RequestContext;
use document::{DocID, Document, NormalValue};
use query::{DocFetcher, DocMutator};
use rapidhash::RapidHashSet;
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::RegolithStore;

fn items_schema() -> CollectionVersion {
    CollectionVersion::new(
        "Items",
        "items-v1",
        "items-collection",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            FieldDescription::new("3", "n", FieldKind::int(), schema::CType::LwwRegister),
        ],
    )
    .with_index(IndexDescription::new("items_n").with_field("n", false))
}

fn item(name: &str, n: i64) -> Document {
    let mut doc = Document::new(items_schema());
    doc.set("name", NormalValue::String(name.to_owned()))
        .unwrap();
    doc.set("n", NormalValue::Int(n)).unwrap();
    doc
}

/// Writers that create and update documents until `done` is set, counting
/// the writes that committed.
async fn write_until(
    mutator: AutoCommitMutator<RegolithStore>,
    writer: usize,
    existing: Arc<Vec<DocID>>,
    done: Arc<AtomicBool>,
    committed: Arc<AtomicUsize>,
) {
    let mut round = 0i64;
    while !done.load(Ordering::Acquire) {
        let name = format!("w{writer}-{round}");
        if mutator.create("Items", item(&name, round)).await.is_ok() {
            committed.fetch_add(1, Ordering::Relaxed);
        }
        let target = &existing[(round as usize + writer) % existing.len()];
        let mut doc = item(&name, round + 1);
        doc.set_id(target.clone());
        let fields = RapidHashSet::from_iter(["n".to_owned()]);
        if mutator.update("Items", doc, fields).await.is_ok() {
            committed.fetch_add(1, Ordering::Relaxed);
        }
        round += 1;
    }
}

/// Truncate succeeds while writers keep creating and updating documents in
/// the collection, and removes every document committed before it started.
#[test]
fn truncate_succeeds_while_writers_create_and_update() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(items_schema()).await.unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        let mut existing = Vec::new();
        for i in 0..200 {
            existing.push(
                mutator
                    .create("Items", item(&format!("seed-{i}"), i))
                    .await
                    .unwrap()
                    .doc_id,
            );
        }

        let existing = Arc::new(existing);
        let done = Arc::new(AtomicBool::new(false));
        let committed = Arc::new(AtomicUsize::new(0));
        let writers: Vec<_> = (0..4)
            .map(|writer| {
                let task = write_until(
                    AutoCommitMutator::new(db.clone()),
                    writer,
                    Arc::clone(&existing),
                    Arc::clone(&done),
                    Arc::clone(&committed),
                );
                workers::spawn(
                    Some(crate::common::worker_io::pool()),
                    "writer",
                    move || task,
                )
                .unwrap()
            })
            .collect();
        let truncated = db
            .truncate_collection_as(&RequestContext::ambient(), "Items", None)
            .await;
        done.store(true, Ordering::Release);
        for writer in writers {
            writer.await.unwrap();
        }
        truncated.unwrap();
        assert!(committed.load(Ordering::Relaxed) > 0);

        let seeds: RapidHashSet<String> = existing.iter().map(DocID::to_string).collect();
        let survivors = AutoCommitFetcher::new(db).get_all("Items").await.unwrap();
        for doc in &survivors {
            let id = doc.id().unwrap().to_string();
            assert!(
                !seeds.contains(&id),
                "a document committed before the truncate survived it: {id}"
            );
        }
    })
    .unwrap()
}
