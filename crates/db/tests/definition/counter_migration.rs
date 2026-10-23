//! The one-time move of counter accumulators into their document values: an
//! increment-only counter keeps the larger of its accumulator and its document
//! value (the P-counter max reconcile, applied once); a counter that allows
//! decrements trusts its accumulator; nothing else under `/data/` moves.

use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use rapidhash::{RapidHashMap, RapidHashSet};
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use storage::corekv::{IterOptions, Key, Store};
use storage::keys::CRDTValueKey;
use storage::merge::{counter_f64, counter_i64};
use storage::RegolithStore;

const VERSION: &str = "tally-v1";
const MARKER: &[u8] = b"s/migration/counter-values/v1";

fn tally() -> CollectionVersion {
    CollectionVersion::new(
        "Tally",
        VERSION,
        "col-tally",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "up", FieldKind::int()).with_crdt_type(CType::PCounter),
            FieldDescription::new("3", "net", FieldKind::int()).with_crdt_type(CType::PnCounter),
            FieldDescription::new("4", "ratio", FieldKind::float64())
                .with_crdt_type(CType::PnCounter),
            FieldDescription::new("5", "name", FieldKind::string()),
        ],
    )
}

async fn create(db: &Arc<DB<RegolithStore>>, json: &str) -> DocID {
    AutoCommitMutator::new(db.clone())
        .create("Tally", Document::from_json_str(json).unwrap())
        .await
        .unwrap()
        .doc_id
}

/// An accumulator as the previous release left it: a stored value with one
/// operand folded on top at read.
enum Legacy {
    Int(i64, i64),
    Float(f64, f64),
}

async fn write_accumulators(store: &RegolithStore, values: &[(&str, &str, Legacy)]) {
    let mut txn = store.new_txn(false).await.unwrap();
    for (doc_id, field, value) in values {
        let key = [
            b"d".as_slice(),
            &CRDTValueKey::new(VERSION, doc_id.as_bytes(), *field).bytes(),
        ]
        .concat();
        match value {
            Legacy::Int(base, operand) => {
                txn.set(&key, &base.to_be_bytes()).await.unwrap();
                txn.merge(&key, &counter_i64(*operand)).await.unwrap();
            }
            Legacy::Float(base, operand) => {
                txn.set(&key, &base.to_be_bytes()).await.unwrap();
                txn.merge(&key, &counter_f64(*operand)).await.unwrap();
            }
        }
    }
    txn.commit().await.unwrap();
}

async fn keys_under(store: &RegolithStore, prefix: &[u8]) -> Vec<Vec<u8>> {
    let txn = store.new_txn(true).await.unwrap();
    let mut iter = txn
        .iterator(
            IterOptions::new()
                .with_prefix(prefix.to_vec())
                .with_keys_only(true),
        )
        .await
        .unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        keys.push(pair.key);
    }
    keys
}

async fn values(db: &Arc<DB<RegolithStore>>) -> RapidHashMap<String, Document> {
    LensedAutoCommitFetcher::new(db.clone())
        .get_all("Tally")
        .await
        .unwrap()
        .into_iter()
        .map(|doc| (doc.id().unwrap().to_string(), doc))
        .collect()
}

#[test]
fn accumulators_move_into_their_documents_once() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(tally()).await.unwrap();
        let behind = create(&db, r#"{"up": 5, "name": "behind"}"#)
            .await
            .to_string();
        let ahead = create(&db, r#"{"up": 5, "name": "ahead"}"#)
            .await
            .to_string();
        let signed = create(&db, r#"{"net": 5, "ratio": 1.5, "name": "signed"}"#)
            .await
            .to_string();
        // A field the document value never stored is set from its accumulator.
        let unset = create(&db, r#"{"name": "unset"}"#).await.to_string();

        let lww = [
            b"d".as_slice(),
            &CRDTValueKey::new(VERSION, behind.as_bytes(), "name").bytes(),
        ]
        .concat();
        let lww_priority = [lww.as_slice(), b"/priority"].concat();
        {
            let mut txn = store.new_txn(false).await.unwrap();
            txn.set(&lww, b"lww value").await.unwrap();
            txn.set(&lww_priority, &[1]).await.unwrap();
            txn.commit().await.unwrap();
        }
        let orphans: Vec<String> = (0..600).map(|n| format!("bae-orphan-{n:04}")).collect();
        let mut legacy: Vec<(&str, &str, Legacy)> = vec![
            // The document value is ahead (increments before #1021 reached only
            // the mirror): an increment-only counter keeps 5.
            (behind.as_str(), "up", Legacy::Int(1, 2)),
            // The accumulator is ahead (a replicated increment reached it first).
            (ahead.as_str(), "up", Legacy::Int(4, 5)),
            // A counter that allows decrements trusts its accumulator.
            (signed.as_str(), "net", Legacy::Int(-1, -2)),
            (signed.as_str(), "ratio", Legacy::Float(2.0, 0.25)),
            (unset.as_str(), "up", Legacy::Int(7, 0)),
        ];
        // More accumulators than one batch visits, for documents that are gone.
        legacy.extend(
            orphans
                .iter()
                .map(|doc| (doc.as_str(), "up", Legacy::Int(1, 1))),
        );
        write_accumulators(&store, &legacy).await;

        let reopened = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        let docs = values(&reopened).await;
        assert_eq!(docs[&behind].get("up"), Some(&NormalValue::Int(5)));
        assert_eq!(docs[&ahead].get("up"), Some(&NormalValue::Int(9)));
        assert_eq!(docs[&signed].get("net"), Some(&NormalValue::Int(-3)));
        assert_eq!(
            docs[&signed].get("ratio"),
            Some(&NormalValue::Float64(2.25))
        );
        assert_eq!(docs[&unset].get("up"), Some(&NormalValue::Int(7)));

        // Every counter accumulator is gone; the LWW value and its metadata stay.
        assert_eq!(
            keys_under(&store, b"d/data/").await,
            vec![lww.clone(), lww_priority]
        );
        assert_eq!(keys_under(&store, MARKER).await.len(), 1);

        // The documents now hold the counters: an increment lands once.
        let mut increment = Document::new();
        increment.set_id(DocID::from_string(&ahead).unwrap());
        increment.set_counter_delta("up".to_string(), NormalValue::Int(1));
        AutoCommitMutator::new(reopened.clone())
            .update(
                "Tally",
                increment,
                RapidHashSet::from_iter(["up".to_string()]),
            )
            .await
            .unwrap();

        // Running it again, even with its completion record gone, changes nothing.
        {
            let mut txn = store.new_txn(false).await.unwrap();
            txn.delete(MARKER).await.unwrap();
            txn.commit().await.unwrap();
        }
        let again = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        let docs = values(&again).await;
        assert_eq!(docs[&behind].get("up"), Some(&NormalValue::Int(5)));
        assert_eq!(docs[&ahead].get("up"), Some(&NormalValue::Int(10)));
        assert_eq!(docs[&signed].get("net"), Some(&NormalValue::Int(-3)));
        assert_eq!(docs[&unset].get("up"), Some(&NormalValue::Int(7)));
        assert_eq!(keys_under(&store, MARKER).await.len(), 1);
    })
    .unwrap()
}

/// A store with no accumulators records completion and moves nothing.
#[test]
fn a_store_without_accumulators_only_records_completion() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(tally()).await.unwrap();
        let doc = create(&db, r#"{"up": 3}"#).await.to_string();
        let reopened = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        assert_eq!(
            values(&reopened).await[&doc].get("up"),
            Some(&NormalValue::Int(3))
        );
        assert_eq!(keys_under(&store, MARKER).await.len(), 1);
    })
    .unwrap()
}
