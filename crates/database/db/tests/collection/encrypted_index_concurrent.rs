//! Encrypted index changes outside a transaction rewrite the collection's
//! definition, so concurrent changes on different fields race on its key.
//! The loser of each race runs again on the committed definition, so with no
//! more contenders than the retry budget allows every change succeeds and
//! none is lost. Each add must read the definition in its transaction: a
//! re-run that read the shared cache could still see the definition from
//! before the winner's commit, and write over it.

use std::sync::Arc;

use db::DB;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::{RegolithStore, Store};

/// One contender per attempt the default budget allows: a change loses at
/// most once to each other contender, so `CONTENDERS - 1` re-runs settle it.
const CONTENDERS: usize = 6;
const ROUNDS: usize = 20;
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

fn users(round: usize) -> CollectionVersion {
    let mut fields = vec![FieldDescription::new(
        "1",
        "_docID",
        FieldKind::doc_id(),
        schema::CType::LwwRegister,
    )];
    for field in 0..CONTENDERS {
        fields.push(FieldDescription::new(
            (2 + field).to_string(),
            format!("f{field}"),
            FieldKind::string(),
            schema::CType::LwwRegister,
        ));
    }
    CollectionVersion::new(
        format!("User{round}"),
        format!("user{round}-v1"),
        format!("col-user{round}"),
        fields,
    )
}

fn put_conflicts(store: &RegolithStore) -> u64 {
    store
        .transaction_stats_handle()
        .expect("regolith reports its transaction statistics")
        .snapshot()
        .conflict_reasons
        .iter()
        .filter(|(reason, _)| reason.ends_with("/put"))
        .map(|(_, count)| count)
        .sum()
}

async fn listed(db: &Arc<DB<RegolithStore>>, collection: &str) -> Vec<String> {
    let txn = db.new_txn(true).await.unwrap();
    let mut fields = db.encrypted_indexes_in(&txn, collection).await.unwrap();
    fields.sort();
    fields
}

#[test]
#[mantra_macros::req_test("defra.txn.indexes.concurrent-encrypted-changes")]
fn concurrent_encrypted_index_adds_on_one_collection_all_land() {
    workers::block_on_timeout(
        async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let db = Arc::new(DB::from_arc(store.clone()).unwrap());
            let before = put_conflicts(&store);
            let expected: Vec<String> = (0..CONTENDERS).map(|field| format!("f{field}")).collect();

            for round in 0..ROUNDS {
                let collection = users(round);
                let name = collection.name.clone();
                db.create_collection(collection).await.unwrap();

                let start = Arc::new(regolith::sync::Barrier::new(CONTENDERS));
                let mut tasks = Vec::new();
                for field in 0..CONTENDERS {
                    let (db, name, start) = (db.clone(), name.clone(), start.clone());
                    tasks.push(
                        workers::spawn(
                            Some(crate::common::worker_io::pool()),
                            "test",
                            move || async move {
                                start.wait().await;
                                db.add_encrypted_index(&name, &format!("f{field}")).await
                            },
                        )
                        .unwrap(),
                    );
                }
                for (field, task) in tasks.into_iter().enumerate() {
                    task.await
                        .unwrap()
                        .unwrap_or_else(|error| panic!("round {round}: add on f{field}: {error}"));
                }

                assert_eq!(listed(&db, &name).await, expected, "round {round}");
            }

            let raced = put_conflicts(&store) - before;
            assert!(
                raced > 0,
                "no add conflicted on the definition in {ROUNDS} rounds of {CONTENDERS}: \
             the adds never overlapped, so the test did not exercise the race"
            );
        },
        DEADLINE,
    )
    .expect("every round finishes before the deadline");
}
